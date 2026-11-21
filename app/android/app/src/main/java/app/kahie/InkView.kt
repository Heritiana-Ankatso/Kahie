package app.kahie

import android.content.Context
import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.os.Handler
import android.os.Looper
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.View
import uniffi.nb_ffi.GeometryKind
import uniffi.nb_ffi.InkTool
import uniffi.nb_ffi.PageDoc
import uniffi.nb_ffi.RenderItem
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min

/** Page canvas: stylus/touch/mouse input, pan and zoom, and drawing of the core's geometry. */
class InkView(context: Context) : View(context) {
    enum class Tool { PEN, HIGHLIGHTER, ERASER, TEXT, SELECT, MATH, PLOT }

    /** Optional low-latency renderer for the stroke in progress (see [JetpackWetInk]). */
    interface WetInk {
        /** Returns false if the stroke could not be started; InkView then draws it itself. */
        fun start(e: MotionEvent, pointerId: Int, highlighter: Boolean, colorArgb: Int, sizePx: Float): Boolean
        fun add(e: MotionEvent, pointerId: Int)
        fun finish(e: MotionEvent, pointerId: Int)
        fun cancel(e: MotionEvent?)
    }

    /** When set, wet ink is drawn by this layer instead of [onDraw]. */
    var wetInk: WetInk? = null

    var doc: PageDoc? = null
        set(value) {
            field = value
            selection = emptyList()
            selBounds = null
            band = null
            paths.clear()
            fitted = false
            refresh()
        }
    var tool = Tool.PEN
    /** Viewer of a read-only shared notebook: touches only pan and zoom. */
    var readOnly = false
    /**
     * Horizontal page scrolling: pages fit the screen and a sideways pan past the edge of the
     * page turns to the next (left) or previous (right) page, like a book.
     */
    var horizontal = false
        set(value) { field = value; fit() }
    /** Turn the page by +1 or -1; returns false when there is no such page. */
    var onPageTurn: (Int) -> Boolean = { false }
    private var slide: android.animation.ValueAnimator? = null
    var penColor = 0xFF1D1D1F.toInt()
    var highlighterColor = 0xFFFFE066.toInt()
    var penWidth = 3.0
    var highlighterWidth = 20.0
    var eraserRadius = 9.0
    var autoShapes = false
    var fingerDraws = true

    /** Called after every committed change (stroke, erase, undo…). */
    var onChanged: () -> Unit = {}

    /** Selected item ids (Select tool); empty when nothing is selected. */
    var selection: List<String> = emptyList()
        private set
    /** Called when the selection changes (size of the new selection). */
    var onSelectionChanged: (Int) -> Unit = {}
    private var selBounds: RectF? = null
    private var band: RectF? = null          // rubber band in page coordinates
    private var dragFrom: FloatArray? = null // page point where a move started
    private var dragDx = 0f
    private var dragDy = 0f
    private val bandPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        color = 0xFF2F5BD3.toInt()
        pathEffect = android.graphics.DashPathEffect(floatArrayOf(8f, 6f), 0f)
    }

    fun clearSelection() {
        if (selection.isEmpty() && band == null) return
        selection = emptyList()
        selBounds = null
        band = null
        onSelectionChanged(0)
        invalidate()
    }

    private fun updateSelBounds() {
        val b = doc?.boundsOf(selection)
        selBounds = if (b == null || b.size < 4) null else RectF(b[0], b[1], b[0] + b[2], b[1] + b[3])
    }

    /** After duplicate/delete from the UI: select `ids` (may be empty). */
    fun select(ids: List<String>) {
        selection = ids
        updateSelBounds()
        onSelectionChanged(ids.size)
        invalidate()
    }

    /** Renders maths blocks (set by the activity). */
    var math: MathRenderer? = null
    private var maths: List<uniffi.nb_ffi.MathBlock> = emptyList()
    private val mathPlaceholder = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        color = 0x55888888
    }

    private var plots: List<uniffi.nb_ffi.PlotBlock> = emptyList()

    // Dragging a free point of a construction (E2.6) with the Plot tool.
    private var dragPlot: String? = null
    private var dragPoint: String? = null
    private var dragLines: List<String>? = null
    private var dragMoved = false

    /** Free point of a plot under (x, y) page units, within ~14 screen px: (plot id, name). */
    private fun freePointAt(x: Float, y: Float): Pair<String, String>? {
        val r = 14f * density / s
        for (p in plots.asReversed()) {
            val m = p.markers.firstOrNull { it.kind == "free" && hypot(it.x - x, it.y - y) <= r } ?: continue
            return p.id to m.label
        }
        return null
    }

    private fun handlePlot(e: MotionEvent) {
        val x = toPageX(e.x)
        val y = toPageY(e.y)
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                val hit = if (readOnly) null else freePointAt(x, y)
                dragPlot = hit?.first
                dragPoint = hit?.second
                dragLines = null
                dragMoved = false
            }
            MotionEvent.ACTION_MOVE -> {
                val id = dragPlot ?: return
                val d = doc ?: return
                // Snap to half units, like a squared exercise book.
                val lines = d.movePlotPoint(id, dragPoint!!, x.toDouble(), y.toDouble(), 0.5) ?: return
                if (lines == dragLines) return
                dragLines = lines
                dragMoved = true
                val preview = d.previewPlot(id, lines) ?: return
                plots = plots.map { if (it.id == id) preview else it }
                invalidate()
            }
            MotionEvent.ACTION_UP -> {
                val id = dragPlot
                val lines = dragLines
                dragPlot = null
                if (id != null && dragMoved && lines != null) {
                    // One undoable, synced change for the whole drag.
                    val p = plots.firstOrNull { it.id == id }
                    if (p != null) runCatching { doc?.setPlot(id, lines, p.x0, p.x1, p.y0, p.y1) }
                    refresh()
                    onChanged()
                } else if (id == null) {
                    onPlotTap(x, y, doc?.plotAt(x.toDouble(), y.toDouble()))
                }
            }
            MotionEvent.ACTION_CANCEL -> {
                dragPlot = null
                refresh()
            }
        }
    }
    private var images: List<uniffi.nb_ffi.ImageBlock> = emptyList()

    /** Plot tool tap at page coordinates, with the id of the plot there (if any). */
    var onPlotTap: (x: Float, y: Float, blockId: String?) -> Unit = { _, _, _ -> }

    /** Maths tool tap at page coordinates, with the id of the maths block there (if any). */
    var onMathTap: (x: Float, y: Float, blockId: String?) -> Unit = { _, _, _ -> }

    /** Text tool tap at page coordinates, with the id of the text block there (if any). */
    var onTextTap: (x: Float, y: Float, blockId: String?) -> Unit = { _, _, _ -> }

    // Text blocks: laid out once per key (a key changes whenever the block does).
    private class CachedText(val block: uniffi.nb_ffi.TextBlock, val layout: android.text.StaticLayout)
    private var texts: List<CachedText> = emptyList()
    private val textLayouts = HashMap<String, CachedText>()

    /** Page size in page units, from the page (A4 portrait until a page is set). */
    private var pageW = 794f
    private var pageH = 1123f
    private val density = resources.displayMetrics.density

    // View transform: screen = page * s + (ox, oy)
    private var s = 1f
    private var ox = 0f
    private var oy = 0f
    private var fitted = false
    private var stylusSeen = false

    private class CachedPath(val path: Path, val paint: Paint)
    private val paths = HashMap<String, CachedPath>()
    private var order: List<String> = emptyList()
    private var hidden: Set<String> = emptySet()

    // Wet ink
    private val wet = ArrayList<FloatArray>() // x, y, pressure (page coords)
    private var snapPreview: CachedPath? = null
    private var drawing = false
    private var fastWet = false // the current stroke is drawn by [wetInk]
    private var erasing = false
    private var lastErase: FloatArray? = null
    private var activePointer = -1
    private var holdX = 0f
    private var holdY = 0f
    private val handler = Handler(Looper.getMainLooper())
    private val holdRunnable = Runnable { trySnap() }

    private val wetPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
    }
    private val pagePaint = Paint().apply { color = Color.WHITE }
    private val shadowPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0x22000000 }
    private val rulePaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val eraserPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        color = 0x88333333.toInt()
        strokeWidth = 1.5f * density
    }
    private var eraserCursor: FloatArray? = null

    // Gestures (fingers when a stylus is in use, or two fingers any time)
    private var panning = false
    private var lastPanX = 0f
    private var lastPanY = 0f
    private val scaleDetector = ScaleGestureDetector(context, object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
        override fun onScale(d: ScaleGestureDetector): Boolean {
            zoomAt(d.focusX, d.focusY, d.scaleFactor)
            return true
        }
    })

    init {
        isFocusable = true
        setBackgroundColor(0xFFE6E3DC.toInt())
    }

    // ---------- core sync ----------

    /** Re-read the item list from the core and build paths for new keys. */
    fun refresh() {
        val d = doc ?: return
        val dims = d.pageDims()
        if (dims[0] != pageW || dims[1] != pageH) {
            pageW = dims[0]
            pageH = dims[1]
            if (width > 0) fit()
        }
        order = d.layout()
        val live = order.toHashSet()
        paths.keys.retainAll(live)
        for (key in order) if (key !in paths) d.geometry(key)?.let { paths[key] = toPath(it) }
        maths = d.mathBlocks()
        plots = d.plotBlocks()
        images = d.imageBlocks()
        val blocks = d.textBlocks()
        textLayouts.keys.retainAll(blocks.map { it.key }.toSet())
        texts = blocks.map { b -> textLayouts.getOrPut(b.key) { CachedText(b, layoutText(b.text, b.width, b.size, b.color)) } }
        invalidate()
    }

    private fun layoutText(text: String, width: Float, size: Float, color: UInt): android.text.StaticLayout {
        val paint = android.text.TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
            textSize = size
            this.color = (color.toInt() and 0xFFFFFF) or (0xFF shl 24)
        }
        return android.text.StaticLayout.Builder.obtain(text, 0, text.length, paint, width.toInt().coerceAtLeast(20)).build()
    }

    /** Height in page units that `text` takes when wrapped to `width` (for new and edited blocks). */
    fun measureText(text: String, width: Float, size: Float): Float = layoutText(text, width, size, 0u).height.toFloat()

    private fun toPath(g: RenderItem): CachedPath {
        val c = g.coords
        val path = Path()
        if (c.size >= 2) {
            path.moveTo(c[0], c[1])
            var i = 2
            while (i + 1 < c.size) { path.lineTo(c[i], c[i + 1]); i += 2 }
            if (g.closed) path.close()
        }
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = (g.color.toInt() and 0xFFFFFF) or (0xFF shl 24)
            alpha = (g.alpha * 255).toInt()
            if (g.alpha < 1f) blendMode = BlendMode.MULTIPLY
            if (g.kind == GeometryKind.FILL) {
                style = Paint.Style.FILL
            } else {
                style = Paint.Style.STROKE
                strokeWidth = g.width
                strokeCap = Paint.Cap.ROUND
                strokeJoin = Paint.Join.ROUND
            }
        }
        return CachedPath(path, paint)
    }

    // ---------- view transform ----------

    fun fit() {
        if (width == 0) return
        s = min((width - 32 * density) / pageW, 1.5f * density)
        if (horizontal) s = min(s, (height - 32 * density) / pageH)
        ox = (width - pageW * s) / 2
        oy = if (horizontal) (height - pageH * s) / 2 else 16 * density
        invalidate()
    }

    /** Space left of the page when it is centred, or the 16 dp margin when it is wider. */
    private fun restGap() = max(16 * density, (width - pageW * s) / 2)

    /** End of a pan in horizontal mode: turn the page or settle back to the page edge. */
    private fun settleHorizontal() {
        val gap = restGap()
        val pulledLeft = (width - (ox + pageW * s)) - gap // past the right edge
        val pulledRight = ox - gap // past the left edge
        val threshold = width * 0.2f
        val dir = when {
            pulledLeft > threshold -> 1
            pulledRight > threshold -> -1
            else -> 0
        }
        if (dir != 0 && onPageTurn(dir)) {
            // The new page slides in from the side the gesture came from.
            fit()
            val target = ox
            animateOx(target + dir * width, target)
            return
        }
        // Back to the nearest edge (or centred if the page is narrower than the view).
        val target = if (pageW * s + 2 * gap <= width) (width - pageW * s) / 2
            else ox.coerceIn(width - gap - pageW * s, gap)
        animateOx(ox, target)
    }

    private fun animateOx(from: Float, to: Float) {
        slide?.cancel()
        if (from == to) { invalidate(); return }
        slide = android.animation.ValueAnimator.ofFloat(from, to).apply {
            duration = 220
            interpolator = android.view.animation.DecelerateInterpolator()
            addUpdateListener { ox = it.animatedValue as Float; invalidate() }
            start()
        }
    }

    private fun zoomAt(fx: Float, fy: Float, factor: Float) {
        val ns = (s * factor).coerceIn(0.15f * density, 8f * density)
        val wx = (fx - ox) / s
        val wy = (fy - oy) / s
        s = ns
        ox = fx - wx * s
        oy = fy - wy * s
        invalidate()
    }

    private fun toPageX(x: Float) = (x - ox) / s
    private fun toPageY(y: Float) = (y - oy) / s

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        if (!fitted) { fit(); fitted = true }
    }

    // ---------- drawing ----------

    override fun onDraw(canvas: Canvas) {
        canvas.save()
        canvas.translate(ox, oy)
        canvas.scale(s, s)
        canvas.drawRect(RectF(2f, 4f, pageW + 2f, pageH + 4f), shadowPaint)
        canvas.drawRect(0f, 0f, pageW, pageH, pagePaint)
        drawTemplate(canvas, doc?.template() ?: "lined")
        doc?.let { PageRenderer.drawBackground(canvas, it) }
        val moving = dragFrom != null && selection.isNotEmpty()
        doc?.let { d ->
            for (img in images) { // under the ink, so it can be annotated
                if (img.key in hidden) continue
                canvas.save()
                if (moving && img.id in selection) canvas.translate(dragDx, dragDy)
                PageRenderer.drawImage(canvas, d, img)
                canvas.restore()
            }
        }
        for (key in order) {
            if (key in hidden) continue
            val p = paths[key] ?: continue
            if (moving && key.substringBefore(':') in selection) {
                canvas.save()
                canvas.translate(dragDx, dragDy)
                canvas.drawPath(p.path, p.paint)
                canvas.restore()
            } else {
                canvas.drawPath(p.path, p.paint)
            }
        }
        for (t in texts) {
            if (t.block.key in hidden) continue
            canvas.save()
            if (moving && t.block.id in selection) canvas.translate(dragDx, dragDy)
            canvas.translate(t.block.x, t.block.y)
            t.layout.draw(canvas)
            canvas.restore()
        }
        for (p in plots) {
            if (p.key in hidden) continue
            canvas.save()
            if (moving && p.id in selection) canvas.translate(dragDx, dragDy)
            PageRenderer.drawPlot(canvas, p, 1f / s * density)
            canvas.restore()
        }
        for (m in maths) {
            if (m.key in hidden) continue
            canvas.save()
            if (moving && m.id in selection) canvas.translate(dragDx, dragDy)
            canvas.translate(m.x, m.y)
            val r = math?.get(m.latex, m.size, (m.color.toInt() and 0xFFFFFF) or (0xFF shl 24))
            val pic = r?.picture
            if (pic != null && r.width > 0f && r.height > 0f) {
                canvas.scale(m.width / r.width, m.height / r.height)
                canvas.scale(r.width / pic.width.coerceAtLeast(1), r.height / pic.height.coerceAtLeast(1))
                canvas.drawPicture(pic)
            } else {
                mathPlaceholder.strokeWidth = 1f / s * density
                canvas.drawRect(0f, 0f, m.width, m.height, mathPlaceholder)
            }
            canvas.restore()
        }
        drawWet(canvas)
        bandPaint.strokeWidth = 1.5f * density / s
        band?.let { canvas.drawRect(it, bandPaint) }
        selBounds?.let {
            val pad = 6f / s * density
            canvas.drawRect(it.left - pad + dragDx, it.top - pad + dragDy, it.right + pad + dragDx, it.bottom + pad + dragDy, bandPaint)
        }
        canvas.restore()
        eraserCursor?.let { canvas.drawCircle(it[0], it[1], eraserRadius.toFloat() * density, eraserPaint) }
    }

    private fun drawTemplate(canvas: Canvas, template: String) {
        rulePaint.strokeWidth = 1f / s * density
        when (template) {
            "lined" -> {
                rulePaint.color = 0xFFC9D8F0.toInt()
                var y = 96f
                while (y < pageH) { canvas.drawLine(0f, y, pageW, y, rulePaint); y += 32f }
                rulePaint.color = 0xFFF0B4B4.toInt()
                canvas.drawLine(80f, 0f, 80f, pageH, rulePaint)
            }
            "grid" -> {
                rulePaint.color = 0xFFDDE3EE.toInt()
                var v = 24f
                while (v < pageW) { canvas.drawLine(v, 0f, v, pageH, rulePaint); v += 24f }
                v = 24f
                while (v < pageH) { canvas.drawLine(0f, v, pageW, v, rulePaint); v += 24f }
            }
            "graph" -> {
                // Millimetre paper: 1 mm ≈ 3.78 page units; darker every 10 mm.
                val mm = 794f / 210f
                var i = 0
                var v = 0f
                while (v < pageW) {
                    rulePaint.color = if (i % 10 == 0) 0xFFB9CBE4.toInt() else 0xFFE4ECF6.toInt()
                    canvas.drawLine(v, 0f, v, pageH, rulePaint); v += mm; i++
                }
                i = 0; v = 0f
                while (v < pageH) {
                    rulePaint.color = if (i % 10 == 0) 0xFFB9CBE4.toInt() else 0xFFE4ECF6.toInt()
                    canvas.drawLine(0f, v, pageW, v, rulePaint); v += mm; i++
                }
            }
            "music" -> {
                rulePaint.color = 0xFF9AA3B2.toInt()
                var top = 80f
                while (top + 40f < pageH - 40f) {
                    for (k in 0 until 5) canvas.drawLine(40f, top + k * 10f, pageW - 40f, top + k * 10f, rulePaint)
                    top += 90f
                }
            }
            "cornell" -> {
                rulePaint.color = 0xFFC9D8F0.toInt()
                var y = 120f
                while (y < pageH - 220f) { canvas.drawLine(0f, y, pageW, y, rulePaint); y += 32f }
                rulePaint.color = 0xFF9AA3B2.toInt()
                canvas.drawLine(220f, 0f, 220f, pageH - 220f, rulePaint)
                canvas.drawLine(0f, pageH - 220f, pageW, pageH - 220f, rulePaint)
            }
            "dots" -> {
                rulePaint.color = 0xFFB8C0CC.toInt()
                val r = 1.3f
                var x = 24f
                while (x < pageW) {
                    var y = 24f
                    while (y < pageH) { canvas.drawCircle(x, y, r, rulePaint); y += 24f }
                    x += 24f
                }
            }
        }
    }

    private fun drawWet(canvas: Canvas) {
        snapPreview?.let { canvas.drawPath(it.path, it.paint); return }
        if (wet.isEmpty() || fastWet) return
        val hl = tool == Tool.HIGHLIGHTER
        wetPaint.color = if (hl) highlighterColor else penColor
        wetPaint.alpha = if (hl) 90 else 255
        val w = if (hl) highlighterWidth else penWidth
        if (wet.size == 1) {
            wetPaint.strokeWidth = radius(w, wet[0][2], hl) * 2
            canvas.drawPoint(wet[0][0], wet[0][1], wetPaint)
            return
        }
        for (i in 1 until wet.size) {
            val a = wet[i - 1]
            val b = wet[i]
            wetPaint.strokeWidth = radius(w, a[2], hl) + radius(w, b[2], hl)
            canvas.drawLine(a[0], a[1], b[0], b[1], wetPaint)
        }
    }

    /** Same pressure curve as nb_ink::radius_at. */
    private fun radius(width: Double, p: Float, highlighter: Boolean): Float {
        if (highlighter) return (width / 2).toFloat()
        val pr = if (p == 0f) 0.5f else p
        return (width / 2 * (0.45 + 0.9 * pr)).toFloat()
    }

    // ---------- input ----------

    override fun onTouchEvent(e: MotionEvent): Boolean {
        val toolType = e.getToolType(e.actionIndex)
        val isStylus = toolType == MotionEvent.TOOL_TYPE_STYLUS || toolType == MotionEvent.TOOL_TYPE_ERASER
        if (isStylus) stylusSeen = true
        val fingerNavOnly = stylusSeen && !fingerDraws

        // Two or more fingers, or fingers while a stylus is in use: pan and zoom.
        if (!drawing && !erasing && (e.pointerCount >= 2 || (toolType == MotionEvent.TOOL_TYPE_FINGER && fingerNavOnly))) {
            return handleGesture(e)
        }
        if ((drawing || erasing) && e.pointerCount >= 2 && toolType == MotionEvent.TOOL_TYPE_FINGER && e.actionMasked == MotionEvent.ACTION_POINTER_DOWN) {
            // A second finger means the user is navigating: drop the stroke.
            cancelInk(e)
            return handleGesture(e)
        }
        if (panning) return handleGesture(e)
        if (readOnly) return handleGesture(e)

        if (tool == Tool.SELECT && !drawing && !erasing && e.pointerCount == 1) {
            handleSelect(e)
            return true
        }
        if (tool == Tool.PLOT && !drawing && !erasing) {
            handlePlot(e)
            return true
        }
        if (tool == Tool.MATH && !drawing && !erasing) {
            if (e.actionMasked == MotionEvent.ACTION_UP) {
                val x = toPageX(e.x)
                val y = toPageY(e.y)
                onMathTap(x, y, doc?.mathAt(x.toDouble(), y.toDouble()))
            }
            return true
        }
        if (tool == Tool.TEXT && !drawing && !erasing) {
            if (e.actionMasked == MotionEvent.ACTION_UP) {
                val x = toPageX(e.x)
                val y = toPageY(e.y)
                onTextTap(x, y, doc?.textAt(x.toDouble(), y.toDouble()))
            }
            return true
        }
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                requestUnbufferedDispatch(e)
                activePointer = e.getPointerId(0)
                val eraser = tool == Tool.ERASER || toolType == MotionEvent.TOOL_TYPE_ERASER ||
                    (e.buttonState and MotionEvent.BUTTON_STYLUS_PRIMARY) != 0
                if (eraser) startErase(e.x, e.y) else startInk(e)
            }
            MotionEvent.ACTION_MOVE -> {
                val idx = e.findPointerIndex(activePointer)
                if (idx < 0) return true
                if (erasing) moveErase(e, idx) else if (drawing) moveInk(e, idx)
            }
            MotionEvent.ACTION_UP -> { if (erasing) endErase() else if (drawing) endInk(e) }
            MotionEvent.ACTION_CANCEL -> cancelInk(e)
        }
        return true
    }

    private fun handleSelect(e: MotionEvent) {
        val d = doc ?: return
        val x = toPageX(e.x)
        val y = toPageY(e.y)
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                val b = selBounds
                val pad = 12f / s * density
                if (b != null && x >= b.left - pad && x <= b.right + pad && y >= b.top - pad && y <= b.bottom + pad) {
                    dragFrom = floatArrayOf(x, y)
                    dragDx = 0f
                    dragDy = 0f
                } else {
                    band = RectF(x, y, x, y)
                    selection = emptyList()
                    selBounds = null
                }
            }
            MotionEvent.ACTION_MOVE -> {
                dragFrom?.let { dragDx = x - it[0]; dragDy = y - it[1] }
                band?.let { it.right = x; it.bottom = y }
            }
            MotionEvent.ACTION_UP -> {
                if (dragFrom != null) {
                    if (dragDx != 0f || dragDy != 0f) {
                        d.moveItems(selection, dragDx.toDouble(), dragDy.toDouble())
                        refresh()
                        onChanged()
                    }
                    dragFrom = null
                    dragDx = 0f
                    dragDy = 0f
                    updateSelBounds()
                } else {
                    band?.let { r ->
                        selection = d.selectRect(r.left.toDouble(), r.top.toDouble(), r.right.toDouble(), r.bottom.toDouble())
                        updateSelBounds()
                        onSelectionChanged(selection.size)
                    }
                    band = null
                }
            }
            MotionEvent.ACTION_CANCEL -> {
                dragFrom = null
                dragDx = 0f
                dragDy = 0f
                band = null
            }
        }
        invalidate()
    }

    /** Quick two-finger tap: undo; three-finger tap: redo (P1-25). */
    var onUndoGesture: () -> Unit = {}
    var onRedoGesture: () -> Unit = {}
    private var tapStart = 0L
    private var tapFingers = 0
    private var tapMoved = 0f
    private var tapScale = 1f

    private fun handleGesture(e: MotionEvent): Boolean {
        if (e.actionMasked == MotionEvent.ACTION_DOWN) slide?.cancel()
        if (!panning) {
            tapStart = e.eventTime
            tapFingers = 0
            tapMoved = 0f
            tapScale = s
        }
        tapFingers = max(tapFingers, e.pointerCount)
        scaleDetector.onTouchEvent(e)
        // Pan with the centroid of all fingers.
        var cx = 0f
        var cy = 0f
        val up = e.actionMasked == MotionEvent.ACTION_POINTER_UP
        var n = 0
        for (i in 0 until e.pointerCount) {
            if (up && i == e.actionIndex) continue
            cx += e.getX(i); cy += e.getY(i); n++
        }
        if (n == 0) return true
        cx /= n; cy /= n
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN, MotionEvent.ACTION_POINTER_UP -> {
                panning = true; lastPanX = cx; lastPanY = cy
            }
            MotionEvent.ACTION_MOVE -> {
                tapMoved += hypot(cx - lastPanX, cy - lastPanY)
                ox += cx - lastPanX; oy += cy - lastPanY
                lastPanX = cx; lastPanY = cy
                invalidate()
            }
            MotionEvent.ACTION_UP -> {
                panning = false
                val quick = e.eventTime - tapStart < 300
                val still = tapMoved < 12 * density && kotlin.math.abs(s / tapScale - 1f) < 0.03f
                if (quick && still) {
                    when (tapFingers) {
                        2 -> if (!readOnly) onUndoGesture()
                        3 -> if (!readOnly) onRedoGesture()
                    }
                } else if (horizontal) {
                    settleHorizontal()
                }
            }
            MotionEvent.ACTION_CANCEL -> panning = false
        }
        return true
    }

    private fun pressureOf(e: MotionEvent, idx: Int, hist: Int = -1): Float {
        val stylus = e.getToolType(idx) == MotionEvent.TOOL_TYPE_STYLUS
        if (!stylus) return 0.5f
        return if (hist >= 0) e.getHistoricalPressure(idx, hist) else e.getPressure(idx)
    }

    private fun startInk(e: MotionEvent) {
        val d = doc ?: return
        val x = toPageX(e.x)
        val y = toPageY(e.y)
        val p = pressureOf(e, 0)
        val hl = tool == Tool.HIGHLIGHTER
        d.beginStroke(
            if (hl) InkTool.HIGHLIGHTER else InkTool.PEN,
            ((if (hl) highlighterColor else penColor) and 0xFFFFFF).toUInt(),
            if (hl) highlighterWidth else penWidth, x.toDouble(), y.toDouble(), p.toDouble(),
        )
        wet.clear()
        wet.add(floatArrayOf(x, y, p))
        snapPreview = null
        drawing = true
        val color = if (hl) (highlighterColor and 0xFFFFFF) or (90 shl 24) else penColor
        val sizePx = ((if (hl) highlighterWidth else penWidth) * s).toFloat()
        fastWet = wetInk?.start(e, activePointer, hl, color, sizePx) ?: false
        holdX = e.x; holdY = e.y
        armHold()
        invalidate()
    }

    private fun moveInk(e: MotionEvent, idx: Int) {
        val d = doc ?: return
        // One FFI call per input event: historical samples plus the current one.
        val n = e.historySize + 1
        val xyp = ArrayList<Double>(n * 3)
        for (h in 0 until e.historySize) {
            xyp.add(toPageX(e.getHistoricalX(idx, h)).toDouble())
            xyp.add(toPageY(e.getHistoricalY(idx, h)).toDouble())
            xyp.add(pressureOf(e, idx, h).toDouble())
        }
        xyp.add(toPageX(e.getX(idx)).toDouble())
        xyp.add(toPageY(e.getY(idx)).toDouble())
        xyp.add(pressureOf(e, idx).toDouble())
        val accepted = d.addPoints(xyp)
        var i = 0
        while (i + 2 < accepted.size) {
            wet.add(floatArrayOf(accepted[i], accepted[i + 1], accepted[i + 2]))
            i += 3
        }
        if (fastWet) wetInk?.add(e, activePointer)
        if (hypot(e.getX(idx) - holdX, e.getY(idx) - holdY) > 4 * density) {
            holdX = e.getX(idx); holdY = e.getY(idx)
            if (snapPreview != null) { d.clearSnap(); snapPreview = null }
            armHold()
        }
        invalidate()
    }

    private fun armHold() {
        handler.removeCallbacks(holdRunnable)
        handler.postDelayed(holdRunnable, 550)
    }

    private fun trySnap() {
        if (!drawing) return
        val g = doc?.snapCurrent() ?: return
        // The snapped shape replaces the raw ink, so InkView draws the rest of this stroke itself.
        if (fastWet) { wetInk?.cancel(null); fastWet = false }
        snapPreview = toPath(g)
        performHapticFeedback(android.view.HapticFeedbackConstants.CLOCK_TICK)
        invalidate()
    }

    private fun endInk(e: MotionEvent) {
        handler.removeCallbacks(holdRunnable)
        drawing = false
        if (fastWet) { wetInk?.finish(e, activePointer); fastWet = false }
        doc?.endStroke((s / density).toDouble(), autoShapes)
        wet.clear()
        snapPreview = null
        refresh()
        onChanged()
    }

    private fun cancelInk(e: MotionEvent? = null) {
        handler.removeCallbacks(holdRunnable)
        if (fastWet) { wetInk?.cancel(e); fastWet = false }
        if (drawing) doc?.cancelStroke()
        if (erasing) doc?.cancelErase()
        drawing = false
        erasing = false
        wet.clear()
        snapPreview = null
        hidden = emptySet()
        eraserCursor = null
        refresh()
    }

    private fun startErase(sx: Float, sy: Float) {
        val d = doc ?: return
        d.beginErase()
        erasing = true
        lastErase = floatArrayOf(toPageX(sx), toPageY(sy))
        eraseTo(sx, sy)
    }

    private fun moveErase(e: MotionEvent, idx: Int) {
        for (h in 0 until e.historySize) eraseTo(e.getHistoricalX(idx, h), e.getHistoricalY(idx, h))
        eraseTo(e.getX(idx), e.getY(idx))
    }

    private fun eraseTo(sx: Float, sy: Float) {
        val d = doc ?: return
        val a = lastErase ?: return
        val bx = toPageX(sx)
        val by = toPageY(sy)
        if (d.eraseSegment(a[0].toDouble(), a[1].toDouble(), bx.toDouble(), by.toDouble(), (eraserRadius * density / s))) {
            hidden = d.erasingKeys().toHashSet()
        }
        lastErase = floatArrayOf(bx, by)
        eraserCursor = floatArrayOf(sx, sy)
        invalidate()
    }

    private fun endErase() {
        erasing = false
        val changed = doc?.endErase() ?: false
        hidden = emptySet()
        eraserCursor = null
        refresh()
        if (changed) onChanged()
    }

    override fun onGenericMotionEvent(e: MotionEvent): Boolean {
        // Mouse wheel / trackpad: scroll, Ctrl+wheel zooms.
        if (e.actionMasked == MotionEvent.ACTION_SCROLL) {
            val v = e.getAxisValue(MotionEvent.AXIS_VSCROLL)
            val hz = e.getAxisValue(MotionEvent.AXIS_HSCROLL)
            if ((e.metaState and android.view.KeyEvent.META_CTRL_ON) != 0) zoomAt(e.x, e.y, if (v > 0) 1.1f else 1 / 1.1f)
            else { oy += v * 60 * density; ox -= hz * 60 * density; invalidate() }
            return true
        }
        return super.onGenericMotionEvent(e)
    }

    fun zoomBy(f: Float) = zoomAt(width / 2f, height / 2f, max(0.1f, f))

    /** Centre of the visible part of the page, in page units (where new images go). */
    fun visibleCentre(): FloatArray = floatArrayOf(toPageX(width / 2f), toPageY(height / 2f))

    /** Scroll (at the current zoom) so the page rectangle x, y, w, h is centred (search results). */
    fun reveal(x: Float, y: Float, w: Float, h: Float) {
        if (width == 0) return
        ox = width / 2f - (x + w / 2) * s
        oy = height / 2f - (y + h / 2) * s
        invalidate()
    }
}
