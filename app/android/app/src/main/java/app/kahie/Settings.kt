package app.kahie

import android.app.Activity
import android.app.AlertDialog
import android.content.SharedPreferences
import android.widget.Button
import android.widget.CheckBox
import android.widget.LinearLayout
import android.widget.RadioButton
import android.widget.RadioGroup
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast

/**
 * *More → Settings*: Simple/Advanced mode (P1-41), toolbar position and left-handed mode
 * (P1-38), theme (P1-39), tips (P1-40) and opt-in crash reports with log export (P1-45).
 */
object Settings {
    const val PREF_THEME = "theme" // "" (system), "light", "dark"

    fun show(a: Activity, prefs: SharedPreferences, relayout: () -> Unit) {
        val pad = a.resources.getDimensionPixelSize(R.dimen.kahie_space_l)
        val box = LinearLayout(a).apply { orientation = LinearLayout.VERTICAL; setPadding(pad, pad / 2, pad, 0) }
        fun heading(id: Int) = TextView(a).apply {
            text = a.getString(id)
            setTextColor(a.getColor(R.color.kahie_on_surface_muted))
            setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, a.resources.getDimension(R.dimen.kahie_text_small))
            isAllCaps = true
            setPadding(0, pad, 0, pad / 4)
            isAccessibilityHeading = true
        }
        fun choice(labels: List<Int>, selected: Int): RadioGroup = RadioGroup(a).apply {
            labels.forEachIndexed { i, l ->
                addView(RadioButton(a).apply { id = 1000 + i; text = a.getString(l); minHeight = a.resources.getDimensionPixelSize(R.dimen.kahie_touch) })
            }
            check(1000 + selected)
        }
        fun RadioGroup.index() = checkedRadioButtonId - 1000

        box.addView(heading(R.string.settings_mode))
        val mode = choice(listOf(R.string.mode_simple, R.string.mode_advanced), if (prefs.getString(Toolbar.PREF_MODE, "simple") == "advanced") 1 else 0)
        box.addView(mode)
        box.addView(TextView(a).apply { text = a.getString(R.string.mode_explain) })

        box.addView(heading(R.string.settings_toolbar))
        val position = choice(listOf(R.string.toolbar_top, R.string.toolbar_side), if (prefs.getString(Toolbar.PREF_POSITION, "top") == "side") 1 else 0)
        box.addView(position)
        val left = CheckBox(a).apply { text = a.getString(R.string.settings_left_handed); isChecked = prefs.getBoolean(Toolbar.PREF_LEFT_HANDED, false) }
        box.addView(left)

        box.addView(heading(R.string.settings_theme))
        val themes = listOf("", "light", "dark")
        val theme = choice(listOf(R.string.theme_system, R.string.theme_light, R.string.theme_dark), themes.indexOf(prefs.getString(PREF_THEME, "")).coerceAtLeast(0))
        box.addView(theme)

        box.addView(heading(R.string.tutorials))
        box.addView(Button(a).apply {
            text = a.getString(R.string.settings_tips_reset)
            setOnClickListener {
                val e = prefs.edit()
                for (k in prefs.all.keys) if (k.startsWith("tip_")) e.remove(k)
                e.apply()
                Toast.makeText(a, R.string.tips_reset_done, Toast.LENGTH_SHORT).show()
            }
        })

        box.addView(heading(R.string.crash_reports))
        val crash = CheckBox(a).apply { text = a.getString(R.string.settings_crash_optin); isChecked = CrashReports.enabled(a) }
        box.addView(crash)
        box.addView(TextView(a).apply { text = a.getString(R.string.crash_reports_explain) })
        val logs = LinearLayout(a).apply { orientation = LinearLayout.HORIZONTAL }
        fun withLog(f: (String) -> Unit) {
            val log = CrashReports.log(a)
            if (log.isEmpty()) Toast.makeText(a, R.string.crash_log_empty, Toast.LENGTH_SHORT).show() else f(log)
        }
        logs.addView(Button(a).apply {
            text = a.getString(R.string.crash_log_copy)
            setOnClickListener {
                withLog { log ->
                    a.getSystemService(android.content.ClipboardManager::class.java)
                        ?.setPrimaryClip(android.content.ClipData.newPlainText("Kahie crash log", log))
                    Toast.makeText(a, R.string.copied, Toast.LENGTH_SHORT).show()
                }
            }
        })
        logs.addView(Button(a).apply {
            text = a.getString(R.string.crash_log_share)
            setOnClickListener {
                withLog { log ->
                    val send = android.content.Intent(android.content.Intent.ACTION_SEND)
                        .setType("text/plain")
                        .putExtra(android.content.Intent.EXTRA_SUBJECT, "Kahie crash log")
                        .putExtra(android.content.Intent.EXTRA_TEXT, log)
                    a.startActivity(android.content.Intent.createChooser(send, a.getString(R.string.crash_log_share)))
                }
            }
        })
        logs.addView(Button(a).apply {
            text = a.getString(R.string.crash_log_delete)
            setOnClickListener {
                CrashReports.clear(a)
                Toast.makeText(a, R.string.crash_logs_deleted, Toast.LENGTH_SHORT).show()
            }
        })
        box.addView(logs)

        AlertDialog.Builder(a)
            .setTitle(R.string.settings)
            .setView(ScrollView(a).apply { addView(box) })
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val before = prefs.all.toMap()
                prefs.edit()
                    .putString(Toolbar.PREF_MODE, if (mode.index() == 1) "advanced" else "simple")
                    .putString(Toolbar.PREF_POSITION, if (position.index() == 1) "side" else "top")
                    .putBoolean(Toolbar.PREF_LEFT_HANDED, left.isChecked)
                    .putString(PREF_THEME, themes[theme.index().coerceIn(0, 2)])
                    .putBoolean(CrashReports.PREF_ENABLED, crash.isChecked)
                    .commit()
                if (!crash.isChecked) CrashReports.clear(a) // turning reports off deletes them
                val layoutKeys = listOf(Toolbar.PREF_MODE, Toolbar.PREF_POSITION, Toolbar.PREF_LEFT_HANDED, PREF_THEME)
                if (layoutKeys.any { before[it] != prefs.all[it] }) relayout()
            }
            .show()
    }
}
