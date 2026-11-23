package app.kahie

import android.graphics.Canvas
import android.graphics.Picture
import android.graphics.RectF
import com.caverock.androidsvg.SVG

/**
 * Inserted SVG pictures. The original SVG text is the asset (content-addressed like photos);
 * it is drawn as a vector [Picture] on screen, in PDF export and in thumbnails, embedded as
 * `data:image/svg+xml` in SVG export, and rasterised only where a bitmap is needed (PNG
 * export, crop/rotate, LaTeX's `\includegraphics` gets a one-page vector PDF instead).
 *
 * Safety: AndroidSVG runs no scripts, and loads nothing outside the file because no
 * external file resolver is registered (fonts, `<image href="http…">` and CSS `@import`
 * are simply skipped). Internal DTD entities are disabled (no entity-expansion bombs).
 */
object SvgImages {
    /** Largest SVG accepted (decompressed), in bytes. */
    const val MAX_BYTES = 8 * 1024 * 1024

    init {
        SVG.setInternalEntitiesEnabled(false)
    }

    /** Whether `bytes` look like an SVG document (UTF-8 text whose root element is `<svg>`). */
    fun isSvg(bytes: ByteArray): Boolean {
        var i = 0
        if (bytes.size >= 3 && bytes[0] == 0xEF.toByte() && bytes[1] == 0xBB.toByte() && bytes[2] == 0xBF.toByte()) i = 3
        while (i < bytes.size && bytes[i].toInt().toChar().isWhitespace()) i++
        if (i >= bytes.size || bytes[i] != '<'.code.toByte()) return false
        // The root element comes after an optional prolog, comments and doctype.
        val head = String(bytes, i, minOf(bytes.size - i, 4096), Charsets.UTF_8)
        return Regex("^(<\\?xml[^>]*>|<!--.*?-->|<!DOCTYPE[^>]*>|\\s)*<svg[\\s>/]", RegexOption.DOT_MATCHES_ALL).containsMatchIn(head)
    }

    fun isGzip(bytes: ByteArray) = bytes.size > 2 && bytes[0] == 0x1F.toByte() && bytes[1] == 0x8B.toByte()

    /** SVG text from SVG or SVGZ bytes, or null if they are neither (or too large). */
    fun normalise(bytes: ByteArray): ByteArray? {
        val plain = if (isGzip(bytes)) {
            runCatching {
                java.util.zip.GZIPInputStream(bytes.inputStream()).use { inp ->
                    val out = java.io.ByteArrayOutputStream()
                    val buf = ByteArray(64 * 1024)
                    while (true) {
                        val n = inp.read(buf)
                        if (n < 0) break
                        out.write(buf, 0, n)
                        if (out.size() > MAX_BYTES) return null
                    }
                    out.toByteArray()
                }
            }.getOrNull() ?: return null
        } else {
            bytes
        }
        if (plain.size > MAX_BYTES) return null
        return if (isSvg(plain)) plain else null
    }

    fun parse(bytes: ByteArray): SVG = SVG.getFromInputStream(bytes.inputStream())

    /**
     * Intrinsic size of an SVG in CSS px: width/height when given in absolute units, else
     * the viewBox, else 300 × 150 (the browser default). Either side may come from the aspect.
     */
    fun size(svg: SVG): Pair<Float, Float> {
        val vb = svg.documentViewBox
        var w = svg.documentWidth
        var h = svg.documentHeight
        val aspect = when {
            vb != null && vb.width() > 0f && vb.height() > 0f -> vb.width() / vb.height()
            w > 0f && h > 0f -> w / h
            else -> 2f
        }
        if (w <= 0f && h <= 0f) {
            if (vb != null && vb.width() > 0f && vb.height() > 0f) { w = vb.width(); h = vb.height() } else { w = 300f; h = 150f }
        } else if (w <= 0f) {
            w = h * aspect
        } else if (h <= 0f) {
            h = w / aspect
        }
        return w to h
    }

    /** Recorded vector drawings by asset hash (assets are immutable). */
    private val pictures = object : android.util.LruCache<String, Picture>(64) {}
    /** Hashes known not to be SVG or not to parse, so they are not retried every frame. */
    private val failed = java.util.Collections.synchronizedSet(HashSet<String>())

    /**
     * The SVG asset `hash` recorded as a [Picture] at its intrinsic size, or null when the
     * asset is not an SVG (or does not parse).
     */
    fun picture(hash: String, bytes: () -> ByteArray?): Picture? {
        pictures.get(hash)?.let { return it }
        if (hash in failed) return null
        val b = bytes()
        val pic = if (b != null && isSvg(b)) runCatching { record(parse(b)) }.getOrNull() else null
        if (pic == null) { if (b != null) failed.add(hash); return null }
        pictures.put(hash, pic)
        return pic
    }

    /** Record `svg` into a picture of its intrinsic size, the viewBox fitted to it. */
    fun record(svg: SVG): Picture {
        val (w, h) = size(svg)
        if (svg.documentViewBox == null && svg.documentWidth > 0f && svg.documentHeight > 0f) {
            svg.setDocumentViewBox(0f, 0f, svg.documentWidth, svg.documentHeight)
        }
        val pw = w.coerceIn(1f, 4096f)
        val ph = h.coerceIn(1f, 4096f)
        svg.setDocumentWidth("100%")
        svg.setDocumentHeight("100%")
        return svg.renderToPicture(kotlin.math.ceil(pw).toInt(), kotlin.math.ceil(ph).toInt())
    }

    /** Draw a recorded picture stretched to `box` (the box keeps the picture's aspect). */
    fun draw(canvas: Canvas, pic: Picture, box: RectF) {
        canvas.save()
        canvas.translate(box.left, box.top)
        canvas.scale(box.width() / pic.width.coerceAtLeast(1), box.height() / pic.height.coerceAtLeast(1))
        canvas.drawPicture(pic)
        canvas.restore()
    }

    /** The SVG rasterised so its longer side is at most `maxPx` (white background left transparent). */
    fun rasterise(pic: Picture, maxPx: Int = 1600): android.graphics.Bitmap {
        val s = maxPx.toFloat() / maxOf(pic.width, pic.height).coerceAtLeast(1)
        val w = (pic.width * s).toInt().coerceAtLeast(1)
        val h = (pic.height * s).toInt().coerceAtLeast(1)
        val bmp = android.graphics.Bitmap.createBitmap(w, h, android.graphics.Bitmap.Config.ARGB_8888)
        draw(Canvas(bmp), pic, RectF(0f, 0f, w.toFloat(), h.toFloat()))
        return bmp
    }

    /** The SVG as a one-page vector PDF of `w` × `h` points (for LaTeX's `\includegraphics`). */
    fun pdf(pic: Picture, w: Float, h: Float): ByteArray {
        val doc = android.graphics.pdf.PdfDocument()
        val pw = kotlin.math.ceil(w).toInt().coerceAtLeast(1)
        val ph = kotlin.math.ceil(h).toInt().coerceAtLeast(1)
        val page = doc.startPage(android.graphics.pdf.PdfDocument.PageInfo.Builder(pw, ph, 1).create())
        draw(page.canvas, pic, RectF(0f, 0f, pw.toFloat(), ph.toFloat()))
        doc.finishPage(page)
        val out = java.io.ByteArrayOutputStream()
        doc.writeTo(out)
        doc.close()
        return out.toByteArray()
    }
}
