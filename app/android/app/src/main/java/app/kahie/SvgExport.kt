package app.kahie

import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.text.StaticLayout
import android.text.TextPaint
import uniffi.nb_ffi.GeometryKind
import uniffi.nb_ffi.PageDoc

/**
 * One page as an SVG document (P1-34), in page units (1 unit = 1/96 in, so the SVG has the
 * page's real size). Everything stays vector except photos and PDF backgrounds, which are
 * embedded as data URIs; maths (MathJax's own SVG) and inserted SVG pictures are embedded as
 * `data:image/svg+xml` images, so they stay vector too.
 */
object SvgExport {
    private fun esc(s: String) = s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\"", "&quot;")
    private fun hex(c: Int) = "#%06x".format(c and 0xFFFFFF)
    private fun n(v: Float) = if (v == v.toLong().toFloat()) v.toLong().toString() else "%.2f".format(java.util.Locale.ROOT, v)
    private fun b64(bytes: ByteArray) = android.util.Base64.encodeToString(bytes, android.util.Base64.NO_WRAP)

    private fun mime(bytes: ByteArray) = when {
        bytes.size > 3 && bytes[0] == 0x89.toByte() && bytes[1] == 'P'.code.toByte() -> "image/png"
        SvgImages.isSvg(bytes) -> "image/svg+xml" // inserted SVGs stay vector
        else -> "image/jpeg"
    }

    /**
     * Records the simple shapes [PageRenderer] draws for paper and plot frames as SVG
     * elements. Paths are not readable on every Android version, so ink and plot curves are
     * written from their coordinates instead (drawPath is ignored here).
     */
    private class Recorder(val out: StringBuilder) : Canvas() {
        private fun stroke(p: Paint) = "stroke=\"${hex(p.color)}\" stroke-width=\"${n(p.strokeWidth.coerceAtLeast(0.5f))}\"" +
            if (p.alpha < 255) " stroke-opacity=\"${n(p.alpha / 255f)}\"" else ""

        override fun drawLine(x0: Float, y0: Float, x1: Float, y1: Float, p: Paint) {
            out.append("<line x1=\"${n(x0)}\" y1=\"${n(y0)}\" x2=\"${n(x1)}\" y2=\"${n(y1)}\" ${stroke(p)}/>\n")
        }

        override fun drawCircle(cx: Float, cy: Float, r: Float, p: Paint) {
            out.append("<circle cx=\"${n(cx)}\" cy=\"${n(cy)}\" r=\"${n(r)}\" fill=\"${hex(p.color)}\"/>\n")
        }

        override fun drawRect(l: Float, t: Float, r: Float, b: Float, p: Paint) {
            val fill = if (p.style == Paint.Style.STROKE) "fill=\"none\" ${stroke(p)}" else "fill=\"${hex(p.color)}\""
            out.append("<rect x=\"${n(l)}\" y=\"${n(t)}\" width=\"${n(r - l)}\" height=\"${n(b - t)}\" $fill/>\n")
        }

        override fun drawText(text: String, x: Float, y: Float, p: Paint) {
            val anchor = if (p.textAlign == Paint.Align.CENTER) " text-anchor=\"middle\"" else ""
            out.append("<text x=\"${n(x)}\" y=\"${n(y)}\" font-size=\"${n(p.textSize)}\" fill=\"${hex(p.color)}\" font-family=\"sans-serif\"$anchor>${esc(text)}</text>\n")
        }

        override fun drawColor(color: Int) {}
        override fun drawPath(path: Path, paint: Paint) {}
        override fun clipRect(left: Float, top: Float, right: Float, bottom: Float): Boolean = true
    }

    private fun polyline(out: StringBuilder, c: FloatArray, color: String, width: Float, extra: String = "") {
        if (c.size < 4) return
        out.append("<polyline fill=\"none\" stroke=\"$color\" stroke-width=\"${n(width)}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"$extra points=\"")
        var i = 0
        while (i + 1 < c.size) { out.append(n(c[i])).append(',').append(n(c[i + 1])).append(' '); i += 2 }
        out.append("\"/>\n")
    }

    fun page(d: PageDoc, math: MathRenderer?): String {
        val dims = d.pageDims()
        val (w, h) = dims[0] to dims[1]
        val out = StringBuilder()
        out.append("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n")
        out.append("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"${n(w / 96f * 25.4f)}mm\" height=\"${n(h / 96f * 25.4f)}mm\" viewBox=\"0 0 ${n(w)} ${n(h)}\">\n")
        out.append("<rect width=\"${n(w)}\" height=\"${n(h)}\" fill=\"#ffffff\"/>\n")
        val rec = Recorder(out)
        PageRenderer.drawPaper(rec, d.template(), 0.75f, w, h)
        // A PDF page (kept once per notebook) is embedded as a 2× PNG of the page.
        PdfPages.renderForExport(d, 2f)?.let { (bmp, place) ->
            val png = java.io.ByteArrayOutputStream().also { bmp.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
            bmp.recycle()
            out.append("<image x=\"${n(place.left)}\" y=\"${n(place.top)}\" width=\"${n(place.width())}\" height=\"${n(place.height())}\" href=\"data:image/png;base64,${b64(png)}\"/>\n")
        }
        d.backgroundHash()?.let { hash ->
            val bytes = d.imageBytes(hash) ?: return@let
            val bmp = PageRenderer.bitmap(d, hash) ?: return@let
            val s = minOf(w / bmp.width, h / bmp.height)
            val bw = bmp.width * s
            val bh = bmp.height * s
            out.append("<image x=\"${n((w - bw) / 2)}\" y=\"${n((h - bh) / 2)}\" width=\"${n(bw)}\" height=\"${n(bh)}\" href=\"data:${mime(bytes)};base64,${b64(bytes)}\"/>\n")
        }
        for (img in d.imageBlocks()) {
            val bytes = d.imageBytes(img.hash) ?: continue
            out.append("<image x=\"${n(img.x)}\" y=\"${n(img.y)}\" width=\"${n(img.width)}\" height=\"${n(img.height)}\" preserveAspectRatio=\"none\" href=\"data:${mime(bytes)};base64,${b64(bytes)}\"/>\n")
        }
        for (key in d.layout()) {
            val g = d.geometry(key) ?: continue
            val c = g.coords.toFloatArray()
            val color = hex(g.color.toInt())
            val alpha = if (g.alpha < 1f) " opacity=\"${n(g.alpha)}\" style=\"mix-blend-mode:multiply\"" else ""
            if (g.kind == GeometryKind.FILL) {
                if (c.size < 6) continue
                out.append("<polygon fill=\"$color\"$alpha points=\"")
                var i = 0
                while (i + 1 < c.size) { out.append(n(c[i])).append(',').append(n(c[i + 1])).append(' '); i += 2 }
                out.append("\"/>\n")
            } else {
                polyline(out, c, color, g.width, alpha)
            }
        }
        for (p in d.plotBlocks()) {
            out.append("<rect x=\"${n(p.x)}\" y=\"${n(p.y)}\" width=\"${n(p.width)}\" height=\"${n(p.height)}\" fill=\"#ffffff\"/>\n")
            // Shaded areas (drawPath is not recorded), clipped to the plot box.
            out.append("<g><clipPath id=\"clip-${esc(p.id)}\"><rect x=\"${n(p.x)}\" y=\"${n(p.y)}\" width=\"${n(p.width)}\" height=\"${n(p.height)}\"/></clipPath></g>\n")
            for (sh in p.shades) {
                val c = sh.points
                if (c.size < 6) continue
                out.append("<polygon fill=\"#2f5bd3\" fill-opacity=\"0.2\" clip-path=\"url(#clip-${esc(p.id)})\" points=\"")
                var i = 0
                while (i + 1 < c.size) { out.append(n(c[i])).append(',').append(n(c[i + 1])).append(' '); i += 2 }
                out.append("\"/>\n")
            }
            PageRenderer.drawPlot(rec, p, 0.75f)
            p.curves.forEachIndexed { i, segments ->
                val dashed = p.dashed.getOrElse(i) { false }
                val custom = p.colors.getOrElse(i) { 0u }.toInt()
                val color = if (custom != 0) hex(custom) else if (dashed) "#6b6f78" else hex(PageRenderer.curveColors[i % PageRenderer.curveColors.size])
                val width = p.widths.getOrElse(i) { 0f }.let { if (it > 0f) it else if (dashed) 1.3f else 2f }
                val extra = if (dashed) " stroke-dasharray=\"6 4\" clip-path=\"url(#clip-${esc(p.id)})\"" else " clip-path=\"url(#clip-${esc(p.id)})\""
                for (seg in segments) polyline(out, seg.toFloatArray(), color, width, extra)
            }
        }
        for (a in d.attachmentBlocks()) {
            out.append("<rect x=\"${n(a.x)}\" y=\"${n(a.y)}\" width=\"${n(a.width)}\" height=\"${n(a.height)}\" rx=\"6\" fill=\"#f1f4fa\" stroke=\"#8ea2c8\"/>\n")
            out.append("<text x=\"${n(a.x + a.height * 0.7f)}\" y=\"${n(a.y + a.height * 0.45f)}\" font-family=\"sans-serif\" font-size=\"${n(a.height * 0.3f)}\" fill=\"#1d1d1f\">${esc(a.name)}</text>\n")
            out.append("<text x=\"${n(a.x + a.height * 0.7f)}\" y=\"${n(a.y + a.height * 0.8f)}\" font-family=\"sans-serif\" font-size=\"${n(a.height * 0.22f)}\" fill=\"#6b6f78\">${esc(Attachments.kind(a))}</text>\n")
        }
        for (b in d.textBlocks()) {
            val layout = RichText.layout(b.text, b.spans, b.width, b.size, b.color)
            out.append("<text font-family=\"sans-serif\" font-size=\"${n(b.size)}\" fill=\"${hex(b.color.toInt())}\">")
            for (line in 0 until layout.lineCount) {
                val ls = layout.getLineStart(line)
                val le = layout.getLineEnd(line)
                out.append("<tspan x=\"${n(b.x + layout.getLineLeft(line))}\" y=\"${n(b.y + layout.getLineBaseline(line))}\" xml:space=\"preserve\">")
                // Rich text runs (P1-17): one tspan per run of the same formatting.
                val lineSpans = b.spans.filter { it.start.toInt() < le && it.end.toInt() > ls }
                val cuts = sortedSetOf(ls, le)
                for (sp in lineSpans) { cuts.add(sp.start.toInt().coerceIn(ls, le)); cuts.add(sp.end.toInt().coerceIn(ls, le)) }
                val c = cuts.toList()
                for (i in 0 until c.size - 1) {
                    val run = b.text.substring(c[i], c[i + 1]).trimEnd('\n')
                    if (run.isEmpty()) continue
                    val sp = lineSpans.lastOrNull { it.start.toInt() <= c[i] && it.end.toInt() >= c[i + 1] }
                    if (sp == null) { out.append(esc(run)); continue }
                    val attrs = StringBuilder()
                    if (sp.bold || sp.heading.toInt() > 0) attrs.append(" font-weight=\"bold\"")
                    if (sp.italic) attrs.append(" font-style=\"italic\"")
                    if (sp.underline) attrs.append(" text-decoration=\"underline\"")
                    sp.color?.let { attrs.append(" fill=\"${hex(it.toInt())}\"") }
                    if (sp.heading.toInt() > 0) attrs.append(" font-size=\"${n(b.size * when (sp.heading.toInt()) { 1 -> 1.6f; 2 -> 1.35f; else -> 1.15f })}\"")
                    val bullet = if (sp.list.toInt() == 1 && c[i] == ls && (ls == 0 || b.text[ls - 1] == '\n')) "• " else ""
                    out.append("<tspan$attrs>${esc(bullet + run)}</tspan>")
                }
                out.append("</tspan>")
            }
            out.append("</text>\n")
        }
        for (m in d.mathBlocks()) {
            val r = math?.get(m.latex, m.size, (m.color.toInt() and 0xFFFFFF) or (0xFF shl 24)) ?: continue
            val svg = r.svg ?: continue
            out.append("<image x=\"${n(m.x)}\" y=\"${n(m.y)}\" width=\"${n(m.width)}\" height=\"${n(m.height)}\" preserveAspectRatio=\"none\" href=\"data:image/svg+xml;base64,${b64(svg.toByteArray())}\"/>\n")
        }
        out.append("</svg>\n")
        return out.toString()
    }
}
