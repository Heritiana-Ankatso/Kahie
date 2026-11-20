package app.kahie

import android.app.Activity
import android.app.AlertDialog
import android.content.Context
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.widget.HorizontalScrollView
import android.widget.ImageButton
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import uniffi.nb_ffi.CoreException
import uniffi.nb_ffi.PageDoc
import java.io.File
import java.util.concurrent.Executors

class MainActivity : Activity() {
    private lateinit var ink: InkView
    private lateinit var pageLabel: TextView
    private val toolButtons = HashMap<InkView.Tool, ImageButton>()
    private val swatches = ArrayList<Pair<Int, View>>()
    private lateinit var undoBtn: ImageButton
    private lateinit var redoBtn: ImageButton
    private lateinit var shapesBtn: ImageButton

    private val io = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    private val saveRunnable = Runnable { saveAsync() }
    private val prefs by lazy { getSharedPreferences("kahie", Context.MODE_PRIVATE) }

    private val notebookDir by lazy { File(filesDir, "notebook").apply { mkdirs() } }
    private var pages: MutableList<File> = mutableListOf()
    private var pageIndex = 0
    private var doc: PageDoc? = null

    private val penColors = intArrayOf(0xFF1D1D1F.toInt(), 0xFF2B59C3.toInt(), 0xFFD1495B.toInt(), 0xFF2A9D8F.toInt(), 0xFFE09F3E.toInt())
    private val highlighterColors = intArrayOf(0xFFFFE066.toInt(), 0xFF9BE564.toInt(), 0xFF7CC6FE.toInt(), 0xFFFF99C8.toInt(), 0xFFFFB86B.toInt())
    private val templates = listOf("lined", "grid", "dots", "blank")

    private val dp get() = resources.displayMetrics.density

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ink = InkView(this)
        ink.autoShapes = prefs.getBoolean("autoShapes", false)
        ink.onChanged = {
            updateHistoryButtons()
            main.removeCallbacks(saveRunnable)
            main.postDelayed(saveRunnable, 400)
        }

        val root = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        val bar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setBackgroundColor(0xFFF6F4EF.toInt())
            setPadding((6 * dp).toInt(), (4 * dp).toInt(), (6 * dp).toInt(), (4 * dp).toInt())
        }
        bar.addView(tool(InkView.Tool.PEN, R.drawable.ic_pen, R.string.pen))
        bar.addView(tool(InkView.Tool.HIGHLIGHTER, R.drawable.ic_highlighter, R.string.highlighter))
        bar.addView(tool(InkView.Tool.ERASER, R.drawable.ic_eraser, R.string.eraser))
        bar.addView(divider())
        for (i in penColors.indices) bar.addView(swatch(i))
        bar.addView(divider())
        bar.addView(widthButton(0.6, 7))
        bar.addView(widthButton(1.0, 11))
        bar.addView(widthButton(2.0, 16))
        bar.addView(divider())
        undoBtn = iconButton(R.drawable.ic_undo, R.string.undo) { doc?.undo(); ink.refresh(); ink.onChanged() }
        redoBtn = iconButton(R.drawable.ic_redo, R.string.redo) { doc?.redo(); ink.refresh(); ink.onChanged() }
        bar.addView(undoBtn)
        bar.addView(redoBtn)
        shapesBtn = iconButton(R.drawable.ic_wand, R.string.shapes) {
            ink.autoShapes = !ink.autoShapes
            prefs.edit().putBoolean("autoShapes", ink.autoShapes).apply()
            updateToolbar()
            toast(getString(if (ink.autoShapes) R.string.shapes_on else R.string.shapes_off))
        }
        bar.addView(shapesBtn)
        bar.addView(textButton(R.string.paper) { cycleTemplate() })
        bar.addView(iconButton(R.drawable.ic_fit, R.string.fit) { ink.fit() })
        bar.addView(iconButton(R.drawable.ic_trash, R.string.clear) { confirmClear() })
        bar.addView(divider())
        bar.addView(iconButton(R.drawable.ic_prev, R.string.prev_page) { goToPage(pageIndex - 1) })
        pageLabel = TextView(this).apply {
            setTextColor(0xFF6B6F78.toInt())
            textSize = 14f
            setPadding((4 * dp).toInt(), 0, (4 * dp).toInt(), 0)
        }
        bar.addView(pageLabel)
        bar.addView(iconButton(R.drawable.ic_next, R.string.next_page) { goToPage(pageIndex + 1) })
        bar.addView(iconButton(R.drawable.ic_plus, R.string.add_page) { addPage() })

        val scroller = HorizontalScrollView(this).apply {
            isHorizontalScrollBarEnabled = false
            setBackgroundColor(0xFFF6F4EF.toInt())
            addView(bar)
        }
        root.addView(scroller, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        // Jetpack Ink draws the stroke in progress on top of the page (prototype, on by default).
        val page = android.widget.FrameLayout(this)
        page.addView(ink, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        if (prefs.getBoolean("jetpackInk", true)) {
            val wet = JetpackWetInk(this)
            ink.wetInk = wet
            page.addView(wet.view, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        }
        root.addView(page, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        root.setOnApplyWindowInsetsListener { v, insets ->
            val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
            v.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            scroller.setPadding(0, 0, 0, 0)
            insets
        }
        root.setBackgroundColor(0xFFF6F4EF.toInt())
        setContentView(root)

        pages = notebookDir.listFiles { f -> f.name.endsWith(".json") }?.sortedBy { it.name }?.toMutableList() ?: mutableListOf()
        if (pages.isEmpty()) pages.add(File(notebookDir, pageName(1)))
        goToPage(prefs.getInt("page", 0).coerceIn(0, pages.size - 1))
        updateToolbar()
    }

    override fun onPause() {
        super.onPause()
        main.removeCallbacks(saveRunnable)
        saveNow()
        prefs.edit().putInt("page", pageIndex).apply()
    }

    override fun onDestroy() {
        super.onDestroy()
        io.shutdown()
    }

    // ---------- pages ----------

    private fun pageName(n: Int) = "page-%04d.json".format(n)

    private fun goToPage(i: Int) {
        if (i < 0 || i >= pages.size) return
        main.removeCallbacks(saveRunnable)
        saveNow()
        pageIndex = i
        doc = try {
            PageDoc.open(pages[i].path)
        } catch (e: CoreException) {
            toast(getString(R.string.open_failed, e.message ?: ""))
            PageDoc.newInMemory()
        }
        ink.doc = doc
        pageLabel.text = getString(R.string.page_count, i + 1, pages.size)
        updateHistoryButtons()
    }

    private fun addPage() {
        val next = (pages.maxOfOrNull { it.name.removePrefix("page-").removeSuffix(".json").toIntOrNull() ?: 0 } ?: 0) + 1
        val template = doc?.template() ?: "lined"
        pages.add(File(notebookDir, pageName(next)))
        goToPage(pages.size - 1)
        doc?.setTemplate(template)
        saveAsync()
    }

    private fun cycleTemplate() {
        val d = doc ?: return
        d.setTemplate(templates[(templates.indexOf(d.template()) + 1) % templates.size])
        ink.invalidate()
        saveAsync()
    }

    private fun confirmClear() {
        AlertDialog.Builder(this)
            .setMessage(R.string.clear)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                if (doc?.clear() == true) { ink.refresh(); ink.onChanged() }
            }
            .show()
    }

    private fun saveAsync() {
        val d = doc ?: return
        io.execute { save(d) }
    }

    private fun saveNow() {
        val d = doc ?: return
        io.submit { save(d) }.get()
    }

    private fun save(d: PageDoc) {
        try {
            d.save()
        } catch (e: CoreException) {
            main.post { toast(getString(R.string.save_failed, e.message ?: "")) }
        }
    }

    // ---------- toolbar ----------

    private fun updateToolbar() {
        for ((t, b) in toolButtons) b.isSelected = t == ink.tool
        for ((t, b) in toolButtons) styleSelected(b, t == ink.tool)
        val palette = if (ink.tool == InkView.Tool.HIGHLIGHTER) highlighterColors else penColors
        val current = if (ink.tool == InkView.Tool.HIGHLIGHTER) ink.highlighterColor else ink.penColor
        for ((i, v) in swatches) {
            val c = palette[i]
            (v.background as GradientDrawable).apply {
                setColor(c)
                setStroke((if (c == current) 3 else 1).times(dp).toInt(), if (c == current) 0xFF2B59C3.toInt() else 0xFFDEDAD0.toInt())
            }
        }
        styleSelected(shapesBtn, ink.autoShapes)
        updateHistoryButtons()
    }

    private fun updateHistoryButtons() {
        val d = doc
        undoBtn.isEnabled = d?.canUndo() == true
        redoBtn.isEnabled = d?.canRedo() == true
        undoBtn.alpha = if (undoBtn.isEnabled) 1f else 0.35f
        redoBtn.alpha = if (redoBtn.isEnabled) 1f else 0.35f
    }

    private fun styleSelected(b: View, on: Boolean) {
        b.background = GradientDrawable().apply {
            cornerRadius = 10 * dp
            setColor(if (on) 0xFF2B59C3.toInt() else Color.TRANSPARENT)
        }
        (b as? ImageButton)?.setColorFilter(if (on) Color.WHITE else 0xFF1D1D1F.toInt())
    }

    private fun iconButton(icon: Int, label: Int, onClick: () -> Unit) = ImageButton(this).apply {
        setImageResource(icon)
        contentDescription = getString(label)
        tooltipText = getString(label)
        setColorFilter(0xFF1D1D1F.toInt())
        background = null
        val size = (44 * dp).toInt()
        layoutParams = LinearLayout.LayoutParams(size, size).apply { marginEnd = (2 * dp).toInt() }
        setOnClickListener { onClick() }
    }

    private fun tool(t: InkView.Tool, icon: Int, label: Int) = iconButton(icon, label) {
        ink.tool = t
        updateToolbar()
    }.also { toolButtons[t] = it }

    private fun swatch(i: Int): View {
        val v = View(this).apply {
            background = GradientDrawable().apply { shape = GradientDrawable.OVAL }
            contentDescription = getString(R.string.color) + " " + (i + 1)
            val size = (30 * dp).toInt()
            layoutParams = LinearLayout.LayoutParams(size, size).apply { setMargins((4 * dp).toInt(), 0, (4 * dp).toInt(), 0) }
            setOnClickListener {
                if (ink.tool == InkView.Tool.HIGHLIGHTER) ink.highlighterColor = highlighterColors[i]
                else { ink.penColor = penColors[i]; if (ink.tool == InkView.Tool.ERASER) ink.tool = InkView.Tool.PEN }
                updateToolbar()
            }
        }
        swatches.add(i to v)
        return v
    }

    private fun widthButton(factor: Double, dotPx: Int) = View(this).apply {
        contentDescription = getString(R.string.width)
        val size = (40 * dp).toInt()
        layoutParams = LinearLayout.LayoutParams(size, size)
        val dot = (dotPx * dp)
        background = object : android.graphics.drawable.Drawable() {
            val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF1D1D1F.toInt() }
            override fun draw(c: android.graphics.Canvas) {
                c.drawCircle(bounds.exactCenterX(), bounds.exactCenterY(), dot / 2, paint)
            }
            override fun setAlpha(a: Int) {}
            override fun setColorFilter(f: android.graphics.ColorFilter?) {}
            @Deprecated("Deprecated in Java")
            override fun getOpacity() = android.graphics.PixelFormat.TRANSLUCENT
        }
        setOnClickListener {
            ink.penWidth = 3.0 * factor
            ink.highlighterWidth = 20.0 * factor
            ink.eraserRadius = 9.0 * factor
        }
    }

    private fun textButton(label: Int, onClick: () -> Unit) = TextView(this).apply {
        text = getString(label)
        textSize = 14f
        setTextColor(0xFF1D1D1F.toInt())
        gravity = Gravity.CENTER
        setPadding((10 * dp).toInt(), 0, (10 * dp).toInt(), 0)
        minHeight = (44 * dp).toInt()
        setOnClickListener { onClick() }
    }

    private fun divider() = View(this).apply {
        setBackgroundColor(0xFFDEDAD0.toInt())
        layoutParams = LinearLayout.LayoutParams((1 * dp).toInt(), (28 * dp).toInt()).apply { setMargins((6 * dp).toInt(), 0, (6 * dp).toInt(), 0) }
    }

    private fun toast(msg: String) = Toast.makeText(this, msg, Toast.LENGTH_SHORT).show()
}
