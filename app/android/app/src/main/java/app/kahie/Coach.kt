package app.kahie

import android.app.Activity
import android.app.AlertDialog
import android.content.SharedPreferences
import android.graphics.drawable.GradientDrawable
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView

/**
 * One-time contextual tips and three short interactive tutorials (P1-40, WS8 "Onboarding").
 *
 * Both use a small banner over the bottom of the page that never blocks writing. A **tip**
 * appears once, after the action it is about (never at first launch: P1-37 wants no prompt),
 * and hides itself. A **tutorial** runs on the real page: each step says what to do and moves
 * on when the app sees it done (a stroke added, the eraser chosen, a page added…), so the
 * user learns by doing. Tutorials start only from *More → Tutorials*.
 */
class Coach(private val a: Activity, private val prefs: SharedPreferences, private val state: () -> State) {

    /** What the tutorials look at. */
    data class State(val items: Int, val tool: InkView.Tool, val pages: Int, val notebooks: Int, val pdfPage: Boolean)

    /** Things the app reports. */
    enum class Event { CHANGED, TOOL, UNDO, PAGE, PAGES_OPENED, PDF_IMPORTED, PDF_TEXT_OPENED }

    enum class Tip(val text: Int) {
        STYLE(R.string.tip_style),
        UNDO(R.string.tip_undo),
        SELECT(R.string.tip_select),
        PAGES(R.string.tip_pages),
        PDF(R.string.tip_pdf),
    }

    private class Step(val text: Int, val done: (start: State, now: State, e: Event) -> Boolean)
    private class Tutorial(val title: Int, val steps: List<Step>, val sample: Boolean = false)

    private val tutorials = listOf(
        Tutorial(
            R.string.tut_write_title,
            listOf(
                Step(R.string.tut_write_1) { s, n, _ -> n.items > s.items },
                Step(R.string.tut_write_2) { _, n, _ -> n.tool == InkView.Tool.ERASER },
                Step(R.string.tut_write_3) { s, n, _ -> n.items < s.items },
                Step(R.string.tut_write_4) { s, n, e -> e == Event.UNDO || n.items > s.items },
            ),
        ),
        Tutorial(
            R.string.tut_org_title,
            listOf(
                Step(R.string.tut_org_1) { s, n, _ -> n.pages > s.pages },
                Step(R.string.tut_org_2) { _, _, e -> e == Event.PAGES_OPENED },
                Step(R.string.tut_org_3) { s, n, _ -> n.notebooks > s.notebooks },
            ),
        ),
        Tutorial(
            R.string.tut_pdf_title,
            listOf(
                Step(R.string.tut_pdf_1) { _, n, e -> e == Event.PDF_IMPORTED || n.pdfPage },
                Step(R.string.tut_pdf_2) { s, n, _ -> n.tool == InkView.Tool.HIGHLIGHTER && n.items > s.items },
                Step(R.string.tut_pdf_3) { s, n, _ -> n.tool == InkView.Tool.PEN && n.items > s.items },
                Step(R.string.tut_pdf_4) { s, n, _ -> n.pages > s.pages },
                Step(R.string.tut_pdf_5) { _, _, e -> e == Event.PDF_TEXT_OPENED },
            ),
            sample = true,
        ),
    )

    /** Imports a sample worksheet PDF (set by the activity, for the PDF tutorial). */
    var importSample: () -> Unit = {}

    private var running: Tutorial? = null
    private var step = 0
    private var stepStart: State? = null

    private val main = Handler(Looper.getMainLooper())
    private val hide = Runnable { banner?.visibility = View.GONE }
    private var banner: LinearLayout? = null
    private lateinit var message: TextView
    private lateinit var actions: LinearLayout

    private val dp = a.resources.displayMetrics.density

    /** Put the banner over the bottom of `page`. */
    fun attach(page: FrameLayout) {
        val pad = a.resources.getDimensionPixelSize(R.dimen.kahie_space_m)
        message = TextView(a).apply {
            setTextColor(a.getColor(R.color.kahie_tip_fg))
            setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, a.resources.getDimension(R.dimen.kahie_text_body))
            setPadding(pad, pad, pad, pad)
        }
        actions = LinearLayout(a).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.END }
        val b = LinearLayout(a).apply {
            orientation = LinearLayout.VERTICAL
            background = GradientDrawable().apply {
                cornerRadius = a.resources.getDimension(R.dimen.kahie_radius)
                setColor(a.getColor(R.color.kahie_tip_bg))
            }
            elevation = 6 * dp
            visibility = View.GONE
            // Announced by TalkBack when it appears.
            accessibilityLiveRegion = View.ACCESSIBILITY_LIVE_REGION_POLITE
            addView(message)
            addView(actions)
        }
        val m = a.resources.getDimensionPixelSize(R.dimen.kahie_space_l)
        page.addView(b, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL).apply {
            setMargins(m, m, m, m)
        })
        banner = b
    }

    private fun action(label: Int, onClick: () -> Unit) = TextView(a).apply {
        text = a.getString(label)
        setTextColor(a.getColor(R.color.kahie_tip_action))
        setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, a.resources.getDimension(R.dimen.kahie_text_label))
        isAllCaps = true
        gravity = Gravity.CENTER
        minHeight = a.resources.getDimensionPixelSize(R.dimen.kahie_touch)
        val p = a.resources.getDimensionPixelSize(R.dimen.kahie_space_l)
        setPadding(p, 0, p, 0)
        isClickable = true
        isFocusable = true
        setOnClickListener { onClick() }
    }

    private fun show(text: String, autoHideMs: Long, vararg buttons: Pair<Int, () -> Unit>) {
        val b = banner ?: return
        main.removeCallbacks(hide)
        message.text = text
        actions.removeAllViews()
        for ((label, f) in buttons) actions.addView(action(label, f))
        b.visibility = View.VISIBLE
        if (autoHideMs > 0) main.postDelayed(hide, autoHideMs)
    }

    // ---------- tips ----------

    /** Show tip `t` if it was never shown (and no tutorial is running). */
    fun tip(t: Tip) {
        val key = "tip_${t.name.lowercase()}"
        if (running != null || prefs.getBoolean(key, false)) return
        prefs.edit().putBoolean(key, true).apply()
        show(a.getString(t.text), 9000, R.string.tip_dismiss to { banner?.visibility = View.GONE })
    }

    /** Show every tip again (Settings). */
    fun resetTips() {
        val e = prefs.edit()
        for (t in Tip.values()) e.remove("tip_${t.name.lowercase()}")
        e.apply()
    }

    /** The app reports something: advance a tutorial, or show the tip it calls for. */
    fun event(e: Event) {
        val now = state()
        val tut = running
        if (tut != null) {
            val start = stepStart ?: now
            if (tut.steps[step].done(start, now, e)) {
                step++
                if (step >= tut.steps.size) finish(true) else showStep()
            }
            return
        }
        when {
            e == Event.CHANGED && now.items > 0 && now.tool == InkView.Tool.PEN -> tip(Tip.STYLE)
            e == Event.TOOL && now.tool == InkView.Tool.ERASER -> tip(Tip.UNDO)
            e == Event.PAGE && now.pages >= 2 -> tip(Tip.PAGES)
            e == Event.PDF_IMPORTED -> tip(Tip.PDF)
        }
    }

    // ---------- tutorials ----------

    fun chooseTutorial() {
        AlertDialog.Builder(a)
            .setTitle(R.string.tutorials)
            .setItems(tutorials.map { a.getString(it.title) }.toTypedArray()) { _, i -> start(tutorials[i]) }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    private fun start(t: Tutorial) {
        running = t
        step = 0
        showStep()
    }

    private fun showStep() {
        val t = running ?: return
        stepStart = state()
        val text = a.getString(R.string.tutorial_step, step + 1, t.steps.size, a.getString(t.steps[step].text))
        if (t.sample && step == 0) {
            show(text, 0, R.string.tut_pdf_sample to { importSample() }, R.string.tutorial_skip to { finish(false) })
        } else {
            show(text, 0, R.string.tutorial_skip to { finish(false) })
        }
    }

    private fun finish(completed: Boolean) {
        running = null
        if (completed) show(a.getString(R.string.tutorial_done), 6000, R.string.tip_dismiss to { banner?.visibility = View.GONE })
        else banner?.visibility = View.GONE
    }
}
