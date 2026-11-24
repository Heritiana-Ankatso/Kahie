package app.kahie

import android.app.Activity
import android.app.AlertDialog
import android.content.SharedPreferences
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.HorizontalScrollView
import android.widget.ImageButton
import android.widget.LinearLayout
import android.widget.PopupWindow
import android.widget.ScrollView
import android.widget.SeekBar
import android.widget.TextView

/**
 * The page toolbar (P1-38, P1-41), built from the design tokens (P1-39).
 *
 * - **Simple mode** (default): 7 items — pen, highlighter, eraser, select (lasso), undo,
 *   redo, more — plus the page indicator (previous / page n of m / next / add a page), which
 *   is navigation rather than a tool. Tapping the selected pen or highlighter again opens its
 *   colours and thicknesses; everything else is under *More*.
 * - **Advanced mode**: every tool and action on the bar, colour swatches and widths, and a
 *   full style editor (any colour, exact widths, eraser size, shape tidying).
 * - **Position**: top bar, or side rail. **Left-handed**: the rail goes to the right and the
 *   top bar's order is mirrored, so the tools sit under the free hand.
 *
 * Every control has a spoken label (contentDescription) and a tooltip (P1-46).
 */
class Toolbar(private val a: Activity, private val ink: InkView, private val prefs: SharedPreferences, private val act: Actions) {

    /** What the toolbar's controls do; provided by the activity. */
    class Actions(
        val undo: () -> Unit,
        val redo: () -> Unit,
        val toolChosen: (InkView.Tool) -> Unit,
        val textTool: () -> Unit,
        val mathTool: () -> Unit,
        val plotTool: () -> Unit,
        val selectTool: () -> Unit,
        val selection: () -> Unit,
        val image: () -> Unit,
        val paper: () -> Unit,
        val notebooks: () -> Unit,
        val pages: () -> Unit,
        val fit: () -> Unit,
        val clear: () -> Unit,
        val prevPage: () -> Unit,
        val nextPage: () -> Unit,
        val addPage: () -> Unit,
        val toggleShapes: () -> Unit,
        val pdfText: () -> Unit,
        val tutorials: () -> Unit,
        val settings: () -> Unit,
        val syncStatus: () -> Unit,
        /** Lasso selection (P1-14), tool options: pencil, brush, eraser modes (P1-10/11), search (P1-32). */
        val lasso: () -> Unit = {},
        val toolOptions: () -> Unit = {},
        val search: () -> Unit = {},
        /** Virtual instruments (E2.10). */
        val instruments: () -> Unit = {},
    )

    companion object {
        const val PREF_MODE = "uiMode"            // "simple" | "advanced"
        const val PREF_POSITION = "toolbarPosition" // "top" | "side"
        const val PREF_LEFT_HANDED = "leftHanded"

        val PEN_COLORS = intArrayOf(0xFF1D1D1F.toInt(), 0xFF2B59C3.toInt(), 0xFFD1495B.toInt(), 0xFF2A9D8F.toInt(), 0xFFE09F3E.toInt())
        val HIGHLIGHTER_COLORS = intArrayOf(0xFFFFE066.toInt(), 0xFF9BE564.toInt(), 0xFF7CC6FE.toInt(), 0xFFFF99C8.toInt(), 0xFFFFB86B.toInt())
        /** Spoken names of the palette colours (pen; highlighter) for TalkBack. */
        val PEN_COLOR_NAMES = intArrayOf(R.string.color_black, R.string.color_blue, R.string.color_red, R.string.color_green, R.string.color_orange)
        val HIGHLIGHTER_COLOR_NAMES = intArrayOf(R.string.color_yellow, R.string.color_green, R.string.color_blue, R.string.color_pink, R.string.color_orange)
    }

    val advanced get() = prefs.getString(PREF_MODE, "simple") == "advanced"
    val side get() = prefs.getString(PREF_POSITION, "top") == "side"
    val leftHanded get() = prefs.getBoolean(PREF_LEFT_HANDED, false)

    private val dp = a.resources.displayMetrics.density
    private fun color(id: Int) = a.getColor(id)
    private fun dimen(id: Int) = a.resources.getDimensionPixelSize(id)

    private val toolButtons = HashMap<InkView.Tool, View>()
    private val swatches = ArrayList<Pair<Int, View>>()
    private var shapesBtn: View? = null
    private lateinit var undoBtn: ImageButton
    private lateinit var redoBtn: ImageButton

    lateinit var pageLabel: TextView
        private set
    lateinit var syncDot: TextView
        private set

    /** The toolbar view (scrollable), to add next to the page. */
    fun build(): View {
        toolButtons.clear()
        swatches.clear()
        shapesBtn = null
        val bar = LinearLayout(a).apply {
            orientation = if (side) LinearLayout.VERTICAL else LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
            setBackgroundColor(color(R.color.kahie_surface))
            val p = dimen(R.dimen.kahie_space_s)
            setPadding(p, p, p, p)
        }
        val tools = ArrayList<View>()
        undoBtn = iconButton(R.drawable.ic_undo, R.string.undo) { act.undo() }
        redoBtn = iconButton(R.drawable.ic_redo, R.string.redo) { act.redo() }
        if (advanced) {
            tools += tool(InkView.Tool.PEN, R.drawable.ic_pen, R.string.pen)
            tools += tool(InkView.Tool.HIGHLIGHTER, R.drawable.ic_highlighter, R.string.highlighter)
            tools += tool(InkView.Tool.ERASER, R.drawable.ic_eraser, R.string.eraser)
            tools += divider()
            for (i in PEN_COLORS.indices) tools += swatch(i, swatches)
            tools += divider()
            tools += widthButton(0.6, 7, R.string.width_thin)
            tools += widthButton(1.0, 11, R.string.width_medium)
            tools += widthButton(2.0, 16, R.string.width_thick)
            tools += textButton(R.string.style) { showStyleEditor() }
            tools += textButton(R.string.tool_options) { act.toolOptions() }
            tools += divider()
            tools += undoBtn
            tools += redoBtn
            shapesBtn = iconButton(R.drawable.ic_wand, R.string.shapes) { act.toggleShapes() }
            tools += shapesBtn!!
            tools += tool(InkView.Tool.SELECT, R.drawable.ic_select, R.string.select_tool) { act.selectTool() }
            tools += textButton(R.string.lasso_tool) { act.lasso() }
            tools += textButton(R.string.selection) { act.selection() }
            tools += textButton(R.string.insert) { act.image() }
            tools += textButton(R.string.plot_tool) { act.plotTool() }
            tools += textButton(R.string.math_tool) { act.mathTool() }
            tools += textButton(R.string.text_tool) { act.textTool() }
            tools += textButton(R.string.paper) { act.paper() }
            tools += textButton(R.string.notebooks) { act.notebooks() }
            tools += textButton(R.string.pages) { act.pages() }
            tools += textButton(R.string.search) { act.search() }
            tools += iconButton(R.drawable.ic_fit, R.string.fit) { act.fit() }
            tools += iconButton(R.drawable.ic_trash, R.string.clear) { act.clear() }
            tools += iconButton(R.drawable.ic_more, R.string.more) { showMore(it) }
        } else {
            // Simple mode: exactly 7 items (WS8 principle 3).
            tools += tool(InkView.Tool.PEN, R.drawable.ic_pen, R.string.pen)
            tools += tool(InkView.Tool.HIGHLIGHTER, R.drawable.ic_highlighter, R.string.highlighter)
            tools += tool(InkView.Tool.ERASER, R.drawable.ic_eraser, R.string.eraser)
            tools += tool(InkView.Tool.SELECT, R.drawable.ic_select, R.string.select_tool) { act.selectTool() }
            tools += undoBtn
            tools += redoBtn
            tools += iconButton(R.drawable.ic_more, R.string.more) { showMore(it) }
        }
        // Page navigation (not a tool): ‹ n / m › +, and the sync status dot.
        val nav = ArrayList<View>()
        nav += iconButton(R.drawable.ic_prev, R.string.prev_page) { act.prevPage() }
        pageLabel = TextView(a).apply {
            setTextColor(color(R.color.kahie_on_surface_muted))
            setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, a.resources.getDimension(R.dimen.kahie_text_label))
            gravity = Gravity.CENTER
            val p = dimen(R.dimen.kahie_space_s)
            setPadding(p, 0, p, 0)
            // Read out when it changes ("page 3 of 12").
            accessibilityLiveRegion = View.ACCESSIBILITY_LIVE_REGION_POLITE
        }
        nav += pageLabel
        syncDot = TextView(a).apply {
            text = "●"
            textSize = 14f
            visibility = View.GONE
            gravity = Gravity.CENTER
            minWidth = dimen(R.dimen.kahie_touch) / 2
            minHeight = dimen(R.dimen.kahie_touch)
            contentDescription = a.getString(R.string.sync_status)
            setOnClickListener { act.syncStatus() }
        }
        nav += syncDot
        nav += iconButton(R.drawable.ic_next, R.string.next_page) { act.nextPage() }
        nav += iconButton(R.drawable.ic_plus, R.string.add_page) { act.addPage() }

        // Mirror for left-handed use on a top bar: tools at the right, under the free hand.
        val items = if (!side && leftHanded) nav.reversed() + listOf(divider()) + tools.reversed() else tools + listOf(divider()) + nav
        for (v in items) bar.addView(v)
        update()
        return if (side) {
            ScrollView(a).apply {
                isVerticalScrollBarEnabled = false
                setBackgroundColor(color(R.color.kahie_surface))
                addView(bar, ViewGroup.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT))
            }
        } else {
            HorizontalScrollView(a).apply {
                isHorizontalScrollBarEnabled = false
                setBackgroundColor(color(R.color.kahie_surface))
                addView(bar)
            }
        }
    }

    /** Refresh selected states, swatch rings and undo/redo. */
    fun update() {
        for ((t, b) in toolButtons) styleSelected(b, t == ink.tool)
        val hl = ink.tool == InkView.Tool.HIGHLIGHTER
        val palette = if (hl) HIGHLIGHTER_COLORS else PEN_COLORS
        val names = if (hl) HIGHLIGHTER_COLOR_NAMES else PEN_COLOR_NAMES
        val current = if (hl) ink.highlighterColor else ink.penColor
        for ((i, v) in swatches) ringSwatch(v, palette[i], palette[i] == current, names[i])
        shapesBtn?.let { styleSelected(it, ink.autoShapes) }
    }

    fun updateHistory(canUndo: Boolean, canRedo: Boolean) {
        undoBtn.isEnabled = canUndo
        redoBtn.isEnabled = canRedo
        undoBtn.alpha = if (canUndo) 1f else 0.4f
        redoBtn.alpha = if (canRedo) 1f else 0.4f
    }

    // ---------- menus ----------

    /** *More*: everything that is not on the bar (simple mode) or rarely used (advanced). */
    private fun showMore(anchor: View) {
        val entries = ArrayList<Pair<Int, () -> Unit>>()
        if (!advanced) {
            entries += R.string.text_tool to act.textTool
            entries += R.string.math_tool to act.mathTool
            entries += R.string.plot_tool to act.plotTool
            entries += R.string.insert to act.image
            entries += R.string.lasso_tool to act.lasso
            entries += R.string.selection to act.selection
            entries += R.string.search to act.search
            entries += R.string.tool_options to act.toolOptions
            entries += R.string.paper to act.paper
            entries += R.string.pages to act.pages
            entries += R.string.notebooks to act.notebooks
            entries += R.string.fit to act.fit
            entries += R.string.clear to act.clear
        }
        if (!advanced) entries += R.string.shapes_title to act.toggleShapes
        entries += R.string.instruments to act.instruments
        if (ink.doc?.pdfSource() != null) entries += R.string.pdf_text to act.pdfText
        entries += R.string.tutorials to act.tutorials
        entries += R.string.settings to act.settings
        AlertDialog.Builder(a)
            .setItems(entries.map { a.getString(it.first) }.toTypedArray()) { _, i -> entries[i].second() }
            .show()
    }

    /** Colours and thicknesses of the pen or highlighter (simple mode: tap the tool again). */
    private fun showQuickStyle(anchor: View) {
        val box = LinearLayout(a).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            val p = dimen(R.dimen.kahie_space_m)
            setPadding(p, p, p, p)
            background = GradientDrawable().apply {
                cornerRadius = a.resources.getDimension(R.dimen.kahie_radius)
                setColor(color(R.color.kahie_surface))
                setStroke(dp.toInt(), color(R.color.kahie_divider))
            }
            elevation = 8 * dp
        }
        val popup = PopupWindow(box, ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, true)
        val local = ArrayList<Pair<Int, View>>()
        for (i in PEN_COLORS.indices) box.addView(swatch(i, local) { popup.dismiss() })
        box.addView(divider())
        box.addView(widthButton(0.6, 7, R.string.width_thin) { popup.dismiss() })
        box.addView(widthButton(1.0, 11, R.string.width_medium) { popup.dismiss() })
        box.addView(widthButton(2.0, 16, R.string.width_thick) { popup.dismiss() })
        val hl = ink.tool == InkView.Tool.HIGHLIGHTER
        val palette = if (hl) HIGHLIGHTER_COLORS else PEN_COLORS
        val names = if (hl) HIGHLIGHTER_COLOR_NAMES else PEN_COLOR_NAMES
        val current = if (hl) ink.highlighterColor else ink.penColor
        for ((i, v) in local) ringSwatch(v, palette[i], palette[i] == current, names[i])
        popup.showAsDropDown(anchor)
    }

    /** Advanced mode: full style editor (any colour, exact widths, eraser size, shapes). */
    private fun showStyleEditor() {
        val pad = dimen(R.dimen.kahie_space_l)
        val box = LinearLayout(a).apply { orientation = LinearLayout.VERTICAL; setPadding(pad, pad / 2, pad, 0) }
        val hex = android.widget.EditText(a).apply {
            hint = "#1D1D1F"
            setText("#%06X".format(ink.penColor and 0xFFFFFF))
            setSingleLine()
            contentDescription = a.getString(R.string.style_color)
        }
        fun label(id: Int) = TextView(a).apply { text = a.getString(id); setPadding(0, pad / 2, 0, 0) }
        fun slider(min: Int, max: Int, value: Double, desc: Int) = SeekBar(a).apply {
            this.max = max - min
            progress = (value * 10).toInt().coerceIn(min, max) - min
            contentDescription = a.getString(desc)
            tag = min
        }
        val pen = slider(5, 120, ink.penWidth, R.string.style_pen_width)
        val hl = slider(50, 400, ink.highlighterWidth, R.string.style_highlighter_width)
        val eraser = slider(30, 300, ink.eraserRadius, R.string.style_eraser_size)
        box.addView(label(R.string.style_color)); box.addView(hex)
        box.addView(label(R.string.style_pen_width)); box.addView(pen)
        box.addView(label(R.string.style_highlighter_width)); box.addView(hl)
        box.addView(label(R.string.style_eraser_size)); box.addView(eraser)
        fun value(s: SeekBar) = (s.progress + s.tag as Int) / 10.0
        AlertDialog.Builder(a)
            .setTitle(R.string.style)
            .setView(ScrollView(a).apply { addView(box) })
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                runCatching { Color.parseColor(hex.text.toString().trim()) }.getOrNull()?.let { ink.penColor = it or (0xFF shl 24) }
                ink.penWidth = value(pen)
                ink.highlighterWidth = value(hl)
                ink.eraserRadius = value(eraser)
                update()
            }
            .show()
    }

    // ---------- controls ----------

    private fun styleSelected(b: View, on: Boolean) {
        b.background = GradientDrawable().apply {
            cornerRadius = a.resources.getDimension(R.dimen.kahie_radius)
            setColor(if (on) color(R.color.kahie_accent) else Color.TRANSPARENT)
        }
        (b as? ImageButton)?.setColorFilter(color(if (on) R.color.kahie_on_accent else R.color.kahie_on_surface))
        b.isSelected = on
    }

    private fun iconButton(icon: Int, label: Int, onClick: (View) -> Unit) = ImageButton(a).apply {
        setImageResource(icon)
        contentDescription = a.getString(label)
        tooltipText = a.getString(label)
        setColorFilter(color(R.color.kahie_on_surface))
        background = null
        val size = dimen(R.dimen.kahie_touch)
        layoutParams = LinearLayout.LayoutParams(size, size).apply { setMargins(dimen(R.dimen.kahie_space_xs), dimen(R.dimen.kahie_space_xs), dimen(R.dimen.kahie_space_xs), dimen(R.dimen.kahie_space_xs)) }
        setOnClickListener { onClick(it) }
    }

    /** A tool button; tapping the selected pen or highlighter opens its style (simple mode). */
    private fun tool(t: InkView.Tool, icon: Int, label: Int, onSelect: (() -> Unit)? = null) = iconButton(icon, label) { v ->
        val again = ink.tool == t
        if (onSelect != null) {
            onSelect()
        } else {
            ink.clearSelection()
            ink.tool = t
            if (again && !advanced && (t == InkView.Tool.PEN || t == InkView.Tool.HIGHLIGHTER)) showQuickStyle(v)
        }
        update()
        act.toolChosen(ink.tool)
    }.also { toolButtons[t] = it }

    private fun ringSwatch(v: View, c: Int, selected: Boolean, name: Int) {
        (v.background as GradientDrawable).apply {
            setColor(c)
            setStroke(((if (selected) 3 else 1) * dp).toInt(), color(if (selected) R.color.kahie_accent else R.color.kahie_divider))
        }
        v.contentDescription = a.getString(name)
        v.isSelected = selected
        v.stateDescription = if (selected) a.getString(R.string.selected) else null
    }

    private fun swatch(i: Int, into: MutableList<Pair<Int, View>>, after: () -> Unit = {}): View {
        val v = View(a).apply {
            background = GradientDrawable().apply { shape = GradientDrawable.OVAL }
            val size = dimen(R.dimen.kahie_swatch)
            // 30dp dot inside a 48dp touch target.
            val m = (dimen(R.dimen.kahie_touch) - size) / 2
            layoutParams = LinearLayout.LayoutParams(size, size).apply { setMargins(m / 2, m, m / 2, m) }
            isClickable = true
            isFocusable = true
            setOnClickListener {
                if (ink.tool == InkView.Tool.HIGHLIGHTER) ink.highlighterColor = HIGHLIGHTER_COLORS[i]
                else { ink.penColor = PEN_COLORS[i]; if (ink.tool != InkView.Tool.PEN) ink.tool = InkView.Tool.PEN }
                update()
                act.toolChosen(ink.tool)
                after()
            }
        }
        into.add(i to v)
        return v
    }

    private fun widthButton(factor: Double, dotPx: Int, label: Int, after: () -> Unit = {}) = View(a).apply {
        contentDescription = a.getString(label)
        tooltipText = a.getString(label)
        val size = dimen(R.dimen.kahie_touch)
        layoutParams = LinearLayout.LayoutParams(size, size)
        val dot = dotPx * dp
        val fg = color(R.color.kahie_on_surface)
        background = object : android.graphics.drawable.Drawable() {
            val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply { color = fg }
            override fun draw(c: android.graphics.Canvas) {
                c.drawCircle(bounds.exactCenterX(), bounds.exactCenterY(), dot / 2, paint)
            }
            override fun setAlpha(alpha: Int) {}
            override fun setColorFilter(f: android.graphics.ColorFilter?) {}
            @Deprecated("Deprecated in Java")
            override fun getOpacity() = android.graphics.PixelFormat.TRANSLUCENT
        }
        isClickable = true
        isFocusable = true
        setOnClickListener {
            ink.penWidth = 3.0 * factor
            ink.highlighterWidth = 20.0 * factor
            ink.eraserRadius = 9.0 * factor
            after()
        }
    }

    private fun textButton(label: Int, onClick: () -> Unit) = TextView(a).apply {
        text = a.getString(label)
        setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, a.resources.getDimension(R.dimen.kahie_text_label))
        setTextColor(color(R.color.kahie_on_surface))
        gravity = Gravity.CENTER
        val p = dimen(R.dimen.kahie_space_m)
        setPadding(p, 0, p, 0)
        minHeight = dimen(R.dimen.kahie_touch)
        minWidth = dimen(R.dimen.kahie_touch)
        isClickable = true
        isFocusable = true
        setOnClickListener { onClick() }
    }

    private fun divider() = View(a).apply {
        setBackgroundColor(color(R.color.kahie_divider))
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        val len = dimen(R.dimen.kahie_divider_len)
        val m = dimen(R.dimen.kahie_space_m)
        layoutParams = if (side) {
            LinearLayout.LayoutParams(len, dp.toInt()).apply { setMargins(0, m, 0, m) }
        } else {
            LinearLayout.LayoutParams(dp.toInt(), len).apply { setMargins(m, 0, m, 0) }
        }
    }
}
