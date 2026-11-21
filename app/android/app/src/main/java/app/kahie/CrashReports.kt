package app.kahie

import android.app.ActivityManager
import android.app.ApplicationExitInfo
import android.content.Context
import android.os.Build
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

/**
 * Opt-in crash reports (P1-45, decision D8): **off by default**, nothing leaves the device,
 * and reports contain **no user content**.
 *
 * When the user turns them on (Settings), an uncaught exception is written to
 * `files/crash/` before the app stops: time, app and Android versions, device model, and the
 * exception *types* with their stack frames. Exception **messages are dropped**, because they
 * can quote content (a file name, a notebook title, text being parsed). Native crashes and
 * ANRs reported by Android (`ApplicationExitInfo`) are added as one line each (reason and
 * time, no trace). The user can copy or share the log to send it to support, or delete it.
 */
object CrashReports {
    const val PREF_ENABLED = "crashReports"
    private const val KEEP = 10

    private fun dir(ctx: Context) = File(ctx.filesDir, "crash").apply { mkdirs() }

    fun enabled(ctx: Context) = ctx.getSharedPreferences("kahie", Context.MODE_PRIVATE).getBoolean(PREF_ENABLED, false)

    /** Install the handler (always; it records only when enabled). Idempotent. */
    fun install(ctx: Context) {
        val app = ctx.applicationContext
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        if (previous is Handler) return
        Thread.setDefaultUncaughtExceptionHandler(Handler(app, previous))
    }

    private class Handler(val app: Context, val next: Thread.UncaughtExceptionHandler?) : Thread.UncaughtExceptionHandler {
        override fun uncaughtException(t: Thread, e: Throwable) {
            if (enabled(app)) runCatching { record(app, e) }
            next?.uncaughtException(t, e)
        }
    }

    private fun stamp(ms: Long) = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", Locale.ROOT)
        .apply { timeZone = TimeZone.getTimeZone("UTC") }.format(Date(ms))

    private fun header(ctx: Context): String {
        val version = runCatching { ctx.packageManager.getPackageInfo(ctx.packageName, 0).versionName }.getOrNull() ?: "?"
        return "app=$version android=${Build.VERSION.RELEASE} sdk=${Build.VERSION.SDK_INT} device=${Build.MANUFACTURER} ${Build.MODEL}"
    }

    /** Stack trace of `e` without any message: types and frames only. */
    fun sanitise(e: Throwable): String {
        val out = StringBuilder()
        var cur: Throwable? = e
        var depth = 0
        while (cur != null && depth < 8) {
            out.append(if (depth == 0) "" else "Caused by: ").append(cur.javaClass.name).append('\n')
            for (f in cur.stackTrace.take(40)) out.append("    at ").append(f.className).append('.').append(f.methodName)
                .append('(').append(f.fileName ?: "?").append(':').append(f.lineNumber).append(")\n")
            cur = cur.cause
            depth++
        }
        return out.toString()
    }

    private fun record(ctx: Context, e: Throwable) {
        val now = System.currentTimeMillis()
        File(dir(ctx), "crash-$now.txt").writeText("time=${stamp(now)} ${header(ctx)}\n${sanitise(e)}")
        prune(ctx)
    }

    private fun prune(ctx: Context) {
        dir(ctx).listFiles()?.sortedByDescending { it.name }?.drop(KEEP)?.forEach { it.delete() }
    }

    /** The whole log, newest first, with recent abnormal exits reported by Android. */
    fun log(ctx: Context): String {
        val out = StringBuilder("Kahie crash log (no user content)\n${header(ctx)}\n\n")
        val files = dir(ctx).listFiles()?.sortedByDescending { it.name }.orEmpty()
        for (f in files) out.append(f.readText()).append('\n')
        if (enabled(ctx)) {
            val am = ctx.getSystemService(ActivityManager::class.java)
            val exits = runCatching { am!!.getHistoricalProcessExitReasons(null, 0, 10) }.getOrDefault(emptyList())
            for (x in exits) {
                val reason = when (x.reason) {
                    ApplicationExitInfo.REASON_CRASH -> "crash"
                    ApplicationExitInfo.REASON_CRASH_NATIVE -> "native crash"
                    ApplicationExitInfo.REASON_ANR -> "not responding"
                    ApplicationExitInfo.REASON_LOW_MEMORY -> "low memory"
                    else -> continue
                }
                out.append("exit time=${stamp(x.timestamp)} reason=$reason\n")
            }
        }
        return if (files.isEmpty() && !out.contains("exit time")) "" else out.toString()
    }

    fun clear(ctx: Context) {
        dir(ctx).listFiles()?.forEach { it.delete() }
    }
}
