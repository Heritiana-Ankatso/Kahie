package app.kahie

import android.app.Activity
import android.os.Handler
import android.os.Looper
import android.text.Editable
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.TextWatcher
import android.text.style.BackgroundColorSpan
import android.text.style.StyleSpan
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.RadioButton
import android.widget.RadioGroup
import uniffi.nb_ffi.HitKind
import uniffi.nb_ffi.SearchHit
import uniffi.nb_ffi.SearchIndex
import java.util.concurrent.Executors

/**
 * Search (P1-32): in the open notebook or in all notebooks, over notebook, section and page
 * titles, typed text, maths (LaTeX) and plot expressions; accents and case are ignored.
 * Matching runs in the core ([SearchIndex]); tapping a result opens its page and highlights
 * the block.
 */
class SearchScreen(
    activity: Activity,
    private val index: SearchIndex,
    private val currentDir: String?,
    /** Directory names of all notebooks not in the trash, most recent first. */
    private val allDirs: () -> List<String>,
    private val onOpen: (SearchHit) -> Unit,
) : Screen(activity) {

    private val prefs = activity.getSharedPreferences("kahie", Activity.MODE_PRIVATE)
    private val input = EditText(activity).apply {
        hint = activity.getString(R.string.search_hint)
        setSingleLine()
        imeOptions = EditorInfo.IME_ACTION_SEARCH
    }
    private val everywhere = RadioButton(activity).apply { text = activity.getString(R.string.search_all); id = ID_ALL }
    private val here = RadioButton(activity).apply { text = activity.getString(R.string.search_this_notebook); id = ID_HERE }
    private val main = Handler(Looper.getMainLooper())
    private val run = Runnable { search() }
    private var generation = 0

    init {
        titleView.text = activity.getString(R.string.search)
        action(activity.getString(R.string.lib_back)) { dismiss() }
        header.addView(input)
        val scope = RadioGroup(activity).apply {
            orientation = RadioGroup.HORIZONTAL
            addView(here)
            addView(everywhere)
            check(if (prefs.getBoolean("searchAll", false) || currentDir == null) ID_ALL else ID_HERE)
            setOnCheckedChangeListener { _, id ->
                prefs.edit().putBoolean("searchAll", id == ID_ALL).apply()
                search()
            }
        }
        header.addView(scope)
        input.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) {
                main.removeCallbacks(run)
                main.postDelayed(run, 250)
            }
        })
        input.setOnEditorActionListener { _, _, _ -> main.removeCallbacks(run); search(); true }
        dialog.setOnShowListener {
            input.requestFocus()
            dialog.window?.setSoftInputMode(android.view.WindowManager.LayoutParams.SOFT_INPUT_STATE_VISIBLE)
        }
    }

    private fun search() {
        val q = input.text.toString()
        val gen = ++generation
        val dirs = if (everywhere.isChecked || currentDir == null) allDirs() else listOf(currentDir)
        if (q.isBlank()) { body.removeAllViews(); return }
        worker.execute {
            val t0 = System.nanoTime()
            val result = runCatching { index.search(q, dirs, 200u) }
            val ms = (System.nanoTime() - t0) / 1_000_000
            main.post { if (gen == generation) show(result.getOrDefault(emptyList()), ms) }
        }
    }

    private fun show(hits: List<SearchHit>, ms: Long) {
        body.removeAllViews()
        val count = activity.resources.getQuantityString(R.plurals.search_results, hits.size, hits.size)
        body.addView(label("$count · $ms ms", 12f, MUTED).apply { setPadding(0, px(4), 0, px(8)) })
        var lastDir: String? = null
        for (h in hits) {
            if (h.notebookDir != lastDir) {
                lastDir = h.notebookDir
                body.addView(label(h.notebookTitle.ifEmpty { activity.getString(R.string.untitled) }, 16f, INK, bold = true).apply {
                    setPadding(0, px(12), 0, px(4))
                })
            }
            body.addView(row(h))
            body.addView(divider())
        }
    }

    private fun kindLabel(k: HitKind) = activity.getString(
        when (k) {
            HitKind.NOTEBOOK_TITLE -> R.string.search_kind_notebook
            HitKind.SECTION_TITLE -> R.string.search_kind_section
            HitKind.PAGE_TITLE -> R.string.search_kind_page
            HitKind.TEXT -> R.string.search_kind_text
            HitKind.MATH -> R.string.search_kind_math
            HitKind.PLOT -> R.string.search_kind_plot
        },
    )

    private fun row(h: SearchHit) = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(px(8), px(8), px(8), px(8))
        val where = if (h.pageNumber > 0u) {
            activity.getString(R.string.page_n, h.pageNumber.toInt()) + (if (h.pageTitle.isNotEmpty() && h.kind != HitKind.PAGE_TITLE) " · ${h.pageTitle}" else "")
        } else ""
        addView(label(listOf(kindLabel(h.kind), where).filter { it.isNotEmpty() }.joinToString(" — "), 12f, MUTED))
        val s = SpannableStringBuilder(h.before)
        val start = s.length
        s.append(h.matched)
        s.setSpan(StyleSpan(android.graphics.Typeface.BOLD), start, s.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        s.setSpan(BackgroundColorSpan(HIGHLIGHT), start, s.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        s.append(h.after)
        addView(label(s, 15f).apply { maxLines = 2 })
        isClickable = true
        contentDescription = h.before + h.matched + h.after
        setOnClickListener { dismiss(); onOpen(h) }
    }

    companion object {
        private const val ID_HERE = 0x6b01
        private const val ID_ALL = 0x6b02
        /** One search at a time; the first one may index notebooks not seen before. */
        private val worker = Executors.newSingleThreadExecutor { r -> Thread(r, "kahie-search").apply { isDaemon = true } }
    }
}
