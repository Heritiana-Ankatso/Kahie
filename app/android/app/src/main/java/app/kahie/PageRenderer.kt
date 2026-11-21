package app.kahie

import android.graphics.BlendMode
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.text.StaticLayout
import android.text.TextPaint
import uniffi.nb_ffi.GeometryKind
import uniffi.nb_ffi.PageDoc
import uniffi.nb_ffi.RenderItem

/**
 * Draws a whole page (paper, ink, text, maths) onto any canvas in page units
 * (794 × 1123 for A4; see `PageDoc.pageDims`). Used for export; every element stays vector, so a PDF canvas
 * receives paths and text rather than pixels.
 */
object PageRenderer {
    const val PAGE_W = 794f
    const val PAGE_H = 1123f

    private fun opaque(c: UInt) = (c.toInt() and 0xFFFFFF) or (0xFF shl 24)

    fun inkPaint(g: RenderItem) = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = opaque(g.color)
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

    fun inkPath(g: RenderItem): Path {
        val c = g.coords
        val path = Path()
        if (c.size >= 2) {
            path.moveTo(c[0], c[1])
            var i = 2
            while (i + 1 < c.size) { path.lineTo(c[i], c[i + 1]); i += 2 }
            if (g.closed) path.close()
        }
        return path
    }

    fun drawPaper(canvas: Canvas, template: String, lineWidth: Float, w: Float = PAGE_W, h: Float = PAGE_H) {
        val p = Paint(Paint.ANTI_ALIAS_FLAG).apply { strokeWidth = lineWidth }
        when (template) {
            "lined" -> {
                p.color = 0xFFC9D8F0.toInt()
                var y = 96f
                while (y < h) { canvas.drawLine(0f, y, w, y, p); y += 32f }
                p.color = 0xFFF0B4B4.toInt()
                canvas.drawLine(80f, 0f, 80f, h, p)
            }
            "grid" -> {
                p.color = 0xFFDDE3EE.toInt()
                var v = 24f
                while (v < w) { canvas.drawLine(v, 0f, v, h, p); v += 24f }
                v = 24f
                while (v < h) { canvas.drawLine(0f, v, w, v, p); v += 24f }
            }
            "graph" -> {
                // Millimetre paper: 1 mm ≈ 3.78 page units; darker every 10 mm.
                val mm = 794f / 210f
                var i = 0
                var v = 0f
                while (v < w) {
                    p.color = if (i % 10 == 0) 0xFFB9CBE4.toInt() else 0xFFE4ECF6.toInt()
                    canvas.drawLine(v, 0f, v, h, p); v += mm; i++
                }
                i = 0; v = 0f
                while (v < h) {
                    p.color = if (i % 10 == 0) 0xFFB9CBE4.toInt() else 0xFFE4ECF6.toInt()
                    canvas.drawLine(0f, v, w, v, p); v += mm; i++
                }
            }
            "music" -> {
                p.color = 0xFF9AA3B2.toInt()
                var top = 80f
                while (top + 40f < h - 40f) {
                    for (k in 0 until 5) canvas.drawLine(40f, top + k * 10f, w - 40f, top + k * 10f, p)
                    top += 90f
                }
            }
            "cornell" -> {
                p.color = 0xFFC9D8F0.toInt()
                var y = 120f
                while (y < h - 220f) { canvas.drawLine(0f, y, w, y, p); y += 32f }
                p.color = 0xFF9AA3B2.toInt()
                canvas.drawLine(220f, 0f, 220f, h - 220f, p)
                canvas.drawLine(0f, h - 220f, w, h - 220f, p)
            }
            "dots" -> {
                p.color = 0xFFB8C0CC.toInt()
                var x = 24f
                while (x < w) {
                    var y = 24f
                    while (y < h) { canvas.drawCircle(x, y, 1.3f, p); y += 24f }
                    x += 24f
                }
            }
        }
    }

    val curveColors = intArrayOf(0xFF2F5BD3.toInt(), 0xFFD0453A.toInt(), 0xFF2E9E5B.toInt(), 0xFF8E44AD.toInt(), 0xFFE67E22.toInt())

    /** Draw a function plot: frame, light grid, axes with tick labels, curves. `unit` = 1 screen px in page units. */
    fun drawPlot(canvas: Canvas, p: uniffi.nb_ffi.PlotBlock, unit: Float) {
        val frame = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; color = 0xFFB0B7C3.toInt(); strokeWidth = unit }
        val grid = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFEEF1F5.toInt(); strokeWidth = unit }
        val axis = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF6B6F78.toInt(); strokeWidth = 1.2f * unit }
        val label = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF6B6F78.toInt(); textSize = 11f }
        val l = p.x
        val t = p.y
        val r = p.x + p.width
        val b = p.y + p.height
        fun px(x: Double) = (l + (x - p.x0) / (p.x1 - p.x0) * p.width).toFloat()
        fun py(y: Double) = (t + (p.y1 - y) / (p.y1 - p.y0) * p.height).toFloat()
        canvas.save()
        canvas.clipRect(l, t, r, b)
        canvas.drawColor(Color.WHITE)
        var gx = kotlin.math.ceil(p.x0 / p.xStep) * p.xStep
        if (!p.axes) gx = Double.MAX_VALUE // "axes off": no grid, axes or tick labels
        while (gx <= p.x1) { canvas.drawLine(px(gx), t, px(gx), b, grid); gx += p.xStep }
        var gy = kotlin.math.ceil(p.y0 / p.yStep) * p.yStep
        if (!p.axes) gy = Double.MAX_VALUE
        while (gy <= p.y1) { canvas.drawLine(l, py(gy), r, py(gy), grid); gy += p.yStep }
        val ax = if (p.axes && 0.0 in p.x0..p.x1) px(0.0) else null
        val ay = if (p.axes && 0.0 in p.y0..p.y1) py(0.0) else null
        ay?.let { canvas.drawLine(l, it, r, it, axis) }
        ax?.let { canvas.drawLine(it, t, it, b, axis) }
        fun fmt(v: Double) = if (kotlin.math.abs(v - Math.rint(v)) < 1e-9) Math.rint(v).toLong().toString() else "%.2f".format(v).trimEnd('0').trimEnd('.')
        val labelY = (ay ?: b) - 3f
        gx = if (p.axes) kotlin.math.ceil(p.x0 / p.xStep) * p.xStep else Double.MAX_VALUE
        while (gx <= p.x1) { if (kotlin.math.abs(gx) > 1e-12) canvas.drawText(fmt(gx), px(gx) + 2f, labelY, label); gx += p.xStep }
        val labelX = (ax ?: l) + 3f
        gy = if (p.axes) kotlin.math.ceil(p.y0 / p.yStep) * p.yStep else Double.MAX_VALUE
        while (gy <= p.y1) { if (kotlin.math.abs(gy) > 1e-12) canvas.drawText(fmt(gy), labelX, py(gy) - 2f, label); gy += p.yStep }
        // Shaded areas first (under the curves), with their value.
        val shadeFill = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL; color = 0x332F5BD3 }
        val valueText = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF1D1D1F.toInt(); textSize = 12f; textAlign = Paint.Align.CENTER }
        for (sh in p.shades) {
            val pts = sh.points
            if (pts.size < 6) continue
            val path = Path()
            path.moveTo(pts[0], pts[1])
            var k = 2
            while (k + 1 < pts.size) { path.lineTo(pts[k], pts[k + 1]); k += 2 }
            path.close()
            canvas.drawPath(path, shadeFill)
            canvas.drawText(sh.label, sh.labelX, sh.labelY, valueText)
        }
        p.curves.forEachIndexed { i, segments ->
            val dashed = p.dashed.getOrElse(i) { false }
            val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                style = Paint.Style.STROKE
                color = if (dashed) 0xFF6B6F78.toInt() else curveColors[i % curveColors.size]
                strokeWidth = if (dashed) 1.3f else 2f
                strokeJoin = Paint.Join.ROUND
                strokeCap = Paint.Cap.ROUND
                if (dashed) pathEffect = android.graphics.DashPathEffect(floatArrayOf(6f, 4f), 0f)
            }
            for (seg in segments) {
                if (seg.size < 4) continue
                val path = Path()
                path.moveTo(seg[0], seg[1])
                var k = 2
                while (k + 1 < seg.size) { path.lineTo(seg[k], seg[k + 1]); k += 2 }
                canvas.drawPath(path, paint)
            }
        }
        // Analysis markers (roots, extrema, …) with their coordinates.
        val dot = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
        val markText = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF1D1D1F.toInt(); textSize = 10.5f }
        for (m in p.markers) {
            if (m.kind == "free") {
                // Draggable point: a larger ring, so it reads as a handle.
                dot.color = markerColor(m.kind)
                canvas.drawCircle(m.x, m.y, 4.5f, dot)
                dot.color = Color.WHITE
                canvas.drawCircle(m.x, m.y, 2.2f, dot)
            } else if (m.kind != "label") {
                dot.color = markerColor(m.kind)
                canvas.drawCircle(m.x, m.y, 3f, dot)
            }
            // Keep labels inside the box.
            val w = markText.measureText(m.label)
            val tx = (m.x + 5f).coerceAtMost(r - w - 2f).coerceAtLeast(l + 2f)
            val ty = (m.y - 5f).coerceAtLeast(t + 12f)
            canvas.drawText(m.label, tx, ty, markText)
        }
        canvas.restore()
        canvas.drawRect(l, t, r, b, frame)
    }

    fun markerColor(kind: String): Int = when (kind) {
        "root" -> 0xFF1D1D1F.toInt()
        "max", "min" -> 0xFFD0453A.toInt()
        "inflection" -> 0xFF8E44AD.toInt()
        "intersection" -> 0xFF2E9E5B.toInt()
        else -> 0xFF2F5BD3.toInt()
    }

    /** Decoded images by hash (shared by screen and export; images are immutable). */
    private val bitmaps = object : android.util.LruCache<String, android.graphics.Bitmap>(64 * 1024 * 1024) {
        override fun sizeOf(key: String, value: android.graphics.Bitmap) = value.byteCount
    }

    fun bitmap(d: PageDoc, hash: String): android.graphics.Bitmap? {
        bitmaps.get(hash)?.let { return it }
        val bytes = d.imageBytes(hash) ?: return null
        val bmp = android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: return null
        bitmaps.put(hash, bmp)
        return bmp
    }

    private val imagePaint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)

    fun drawImage(canvas: Canvas, d: PageDoc, img: uniffi.nb_ffi.ImageBlock) {
        val bmp = bitmap(d, img.hash) ?: return
        canvas.drawBitmap(bmp, null, android.graphics.RectF(img.x, img.y, img.x + img.width, img.y + img.height), imagePaint)
    }

    /** The page background (imported PDF page), fitted to the page and centred. */
    fun drawBackground(canvas: Canvas, d: PageDoc) {
        val hash = d.backgroundHash() ?: return
        val bmp = bitmap(d, hash) ?: return
        val (pw, ph) = d.pageDims().let { it[0] to it[1] }
        val scale = minOf(pw / bmp.width, ph / bmp.height)
        val w = bmp.width * scale
        val h = bmp.height * scale
        val l = (pw - w) / 2
        val t = (ph - h) / 2
        canvas.drawBitmap(bmp, null, android.graphics.RectF(l, t, l + w, t + h), imagePaint)
    }

    /** All maths of a page, so an export can wait until every formula is rendered. */
    fun mathRequests(d: PageDoc): List<Triple<String, Float, Int>> =
        d.mathBlocks().map { Triple(it.latex, it.size, opaque(it.color)) }

    /** Draw page `d` (white paper, background, items in order) in page units. */
    fun drawPage(canvas: Canvas, d: PageDoc, math: MathRenderer?, paper: Boolean = true) {
        canvas.drawColor(Color.WHITE)
        if (paper) {
            val dims = d.pageDims()
            drawPaper(canvas, d.template(), 0.75f, dims[0], dims[1])
        }
        drawBackground(canvas, d)
        for (img in d.imageBlocks()) drawImage(canvas, d, img) // under the ink
        for (key in d.layout()) {
            val g = d.geometry(key) ?: continue
            canvas.drawPath(inkPath(g), inkPaint(g))
        }
        for (p in d.plotBlocks()) drawPlot(canvas, p, 0.75f)
        for (b in d.textBlocks()) {
            val tp = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = b.size; color = opaque(b.color) }
            val layout = StaticLayout.Builder.obtain(b.text, 0, b.text.length, tp, b.width.toInt().coerceAtLeast(20)).build()
            canvas.save()
            canvas.translate(b.x, b.y)
            layout.draw(canvas)
            canvas.restore()
        }
        for (m in d.mathBlocks()) {
            val r = math?.get(m.latex, m.size, opaque(m.color)) ?: continue
            val pic = r.picture ?: continue
            canvas.save()
            canvas.translate(m.x, m.y)
            canvas.scale(m.width / pic.width.coerceAtLeast(1), m.height / pic.height.coerceAtLeast(1))
            canvas.drawPicture(pic)
            canvas.restore()
        }
    }
}
