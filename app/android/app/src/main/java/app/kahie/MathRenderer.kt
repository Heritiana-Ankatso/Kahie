package app.kahie

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Picture
import android.os.Handler
import android.os.Looper
import android.webkit.JavascriptInterface
import android.webkit.WebView
import com.caverock.androidsvg.SVG
import org.json.JSONObject

/**
 * LaTeX → vector picture, offline. A hidden WebView runs the same MathJax as Kahie Web
 * (bundled from app/web/vendor/mathjax) to produce SVG; AndroidSVG turns it into a
 * [Picture] that the page canvas draws at any zoom without blurring.
 */
class MathRenderer(context: Context, private val onRendered: () -> Unit) {
    class Result(val picture: Picture?, val width: Float, val height: Float, val error: Boolean)

    private val main = Handler(Looper.getMainLooper())
    private val cache = HashMap<String, Result>()
    private val pending = HashMap<String, MutableList<(Result) -> Unit>>()
    private val queued = ArrayList<Pair<String, String>>() // key, js call, until MathJax is loaded
    private var loaded = false

    @SuppressLint("SetJavaScriptEnabled")
    private val web = WebView(context.applicationContext).apply {
        settings.javaScriptEnabled = true
        settings.allowFileAccess = true // file:///android_asset only; the page loads no network content
        addJavascriptInterface(Bridge(), "Kahie")
        loadUrl("file:///android_asset/math/render.html")
    }

    private fun key(latex: String, size: Float, color: Int) = "$size|$color|$latex"

    /** Cached result, or null while rendering (then [onRendered] fires when it is ready). */
    fun get(latex: String, size: Float, color: Int): Result? {
        val k = key(latex, size, color)
        cache[k]?.let { return it }
        request(latex, size, color) { onRendered() }
        return null
    }

    /** Render (or reuse) and call [done] on the main thread. */
    fun request(latex: String, size: Float, color: Int, done: (Result) -> Unit) {
        val k = key(latex, size, color)
        cache[k]?.let { done(it); return }
        val waiting = pending[k]
        if (waiting != null) { waiting.add(done); return }
        pending[k] = mutableListOf(done)
        val css = "#%06X".format(color and 0xFFFFFF)
        val js = "render(${JSONObject.quote(k)}, ${JSONObject.quote(latex)}, $size, ${JSONObject.quote(css)})"
        if (loaded) web.evaluateJavascript(js, null) else queued.add(k to js)
    }

    private fun finish(k: String, r: Result) {
        cache[k] = r
        pending.remove(k)?.forEach { it(r) }
    }

    private inner class Bridge {
        @JavascriptInterface
        fun onLoaded() = main.post {
            loaded = true
            queued.forEach { (_, js) -> web.evaluateJavascript(js, null) }
            queued.clear()
        }.let { }

        @JavascriptInterface
        fun onSvg(key: String, svg: String, w: Double, h: Double, error: Boolean) = main.post {
            val picture = runCatching { SVG.getFromString(svg).renderToPicture() }.getOrNull()
            finish(key, Result(picture, w.toFloat(), h.toFloat(), error || picture == null))
        }.let { }

        @JavascriptInterface
        fun onFail(key: String, message: String) = main.post {
            finish(key, Result(null, 0f, 0f, true))
        }.let { }
    }
}
