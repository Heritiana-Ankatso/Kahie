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
 * (794 × 1123 for A4). Used for export; every element stays vector, so a PDF canvas
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

    fun drawPaper(canvas: Canvas, template: String, lineWidth: Float) {
        val p = Paint(Paint.ANTI_ALIAS_FLAG).apply { strokeWidth = lineWidth }
        when (template) {
            "lined" -> {
                p.color = 0xFFC9D8F0.toInt()
                var y = 96f
                while (y < PAGE_H) { canvas.drawLine(0f, y, PAGE_W, y, p); y += 32f }
                p.color = 0xFFF0B4B4.toInt()
                canvas.drawLine(80f, 0f, 80f, PAGE_H, p)
            }
            "grid" -> {
                p.color = 0xFFDDE3EE.toInt()
                var v = 24f
                while (v < PAGE_W) { canvas.drawLine(v, 0f, v, PAGE_H, p); v += 24f }
                v = 24f
                while (v < PAGE_H) { canvas.drawLine(0f, v, PAGE_W, v, p); v += 24f }
            }
            "dots" -> {
                p.color = 0xFFB8C0CC.toInt()
                var x = 24f
                while (x < PAGE_W) {
                    var y = 24f
                    while (y < PAGE_H) { canvas.drawCircle(x, y, 1.3f, p); y += 24f }
                    x += 24f
                }
            }
        }
    }

    /** All maths of a page, so an export can wait until every formula is rendered. */
    fun mathRequests(d: PageDoc): List<Triple<String, Float, Int>> =
        d.mathBlocks().map { Triple(it.latex, it.size, opaque(it.color)) }

    /** Draw page `d` (white paper, background, items in order) in page units. */
    fun drawPage(canvas: Canvas, d: PageDoc, math: MathRenderer?, paper: Boolean = true) {
        canvas.drawColor(Color.WHITE)
        if (paper) drawPaper(canvas, d.template(), 0.75f)
        for (key in d.layout()) {
            val g = d.geometry(key) ?: continue
            canvas.drawPath(inkPath(g), inkPaint(g))
        }
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
