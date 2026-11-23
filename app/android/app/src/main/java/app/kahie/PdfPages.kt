package app.kahie

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.pdf.PdfRenderer
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.util.LruCache
import uniffi.nb_ffi.PageDoc
import uniffi.nb_ffi.pdfFile
import java.io.File
import java.util.concurrent.Executors
import kotlin.math.ceil
import kotlin.math.ln
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.roundToInt

/**
 * PDF pages rendered on demand (P1-21, `docs/spikes/p1-20-pdf.md`).
 *
 * The PDF of a notebook is stored once (core: `pdf_import`, `pdf_file`); a page only names
 * the PDF (`PageDoc.pdfSource`). Pages are rendered by Android's `PdfRenderer` on one
 * background thread (it is not thread-safe): a whole-page bitmap at the zoom in use
 * (bucketed by powers of √2, long side ≤ [MAX_PAGE_PX]) and, when zoomed in beyond it, a
 * detail tile of the visible region only. Until a render is ready the best bitmap already
 * cached (or a light placeholder) is drawn, and [draw]'s callback asks for a redraw.
 */
object PdfPages {
    /** Notebook directory whose PDFs are drawn (set when a notebook is opened). */
    @Volatile var notebookDir: String? = null

    private const val MAX_PAGE_PX = 2048
    private const val MAX_TILE_PX = 2600

    private class Doc(val fd: ParcelFileDescriptor, val renderer: PdfRenderer)

    private val worker = Executors.newSingleThreadExecutor { r -> Thread(r, "kahie-pdf").apply { isDaemon = true } }
    private val main = Handler(Looper.getMainLooper())
    private val docs = HashMap<String, Doc>() // worker thread only
    private val pending = HashSet<String>()    // main thread only

    private val cache = object : LruCache<String, Bitmap>((Runtime.getRuntime().maxMemory() / 8).toInt()) {
        override fun sizeOf(key: String, value: Bitmap) = value.allocationByteCount
    }
    /** Largest whole-page width rendered so far for "hash#page" (fallback while rendering). */
    private val best = HashMap<String, Int>()
    private var tile: Pair<String, Bitmap>? = null
    private var tileRect: RectF? = null

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private val placeholder = Paint().apply { color = 0xFFF2F2F2.toInt() }

    /** Close every open PDF (notebook switched). */
    fun reset() {
        main.removeCallbacksAndMessages(null)
        pending.clear()
        best.clear()
        tile = null
        cache.evictAll()
        worker.execute {
            for (d in docs.values) runCatching { d.renderer.close(); d.fd.close() }
            docs.clear()
        }
    }

    private fun open(hash: String): Doc? {
        docs[hash]?.let { return it }
        val dir = notebookDir ?: return null
        val path = pdfFile(dir, hash) ?: return null
        return runCatching {
            val fd = ParcelFileDescriptor.open(File(path), ParcelFileDescriptor.MODE_READ_ONLY)
            Doc(fd, PdfRenderer(fd)).also { docs[hash] = it }
        }.getOrNull()
    }

    /** Where the PDF page is drawn on page `d` (fitted and centred), in page units. */
    fun placement(d: PageDoc, pdfW: Float, pdfH: Float): RectF {
        val dims = d.pageDims()
        val scale = min(dims[0] / pdfW, dims[1] / pdfH)
        val w = pdfW * scale
        val h = pdfH * scale
        val l = (dims[0] - w) / 2
        val t = (dims[1] - h) / 2
        return RectF(l, t, l + w, t + h)
    }

    /** Page aspect (width / height) per "hash#page", known after the first render. */
    private val aspects = HashMap<String, Float>()

    /**
     * Draw the PDF page shown by `d`, if any, in page units. `pxPerUnit` is the current zoom
     * (screen px per page unit) and `visible` the page-unit region on screen (null: whole
     * page). Returns false when the page shows no PDF.
     */
    fun draw(canvas: Canvas, d: PageDoc, pxPerUnit: Float, visible: RectF?, onReady: () -> Unit): Boolean {
        val src = d.pdfSource() ?: return false
        val id = "${src.asset}#${src.pageIndex}"
        val dims = d.pageDims()
        val aspect = aspects[id] ?: (dims[0] / dims[1])
        val place = placement(d, aspect, 1f)
        // Width bucket: powers of √2 around what the screen needs.
        val need = place.width() * pxPerUnit
        val bucket = 2.0.pow(ceil(ln(max(need, 64f).toDouble()) / ln(2.0) * 2) / 2).roundToInt()
        val longSide = max(1f, 1f / aspect)
        val pageWidth = min(bucket, (MAX_PAGE_PX / longSide).toInt())
        val key = "$id@$pageWidth"
        val bmp = cache.get(key) ?: best[id]?.let { cache.get("$id@$it") }
        if (bmp != null) canvas.drawBitmap(bmp, null, place, paint) else canvas.drawRect(place, placeholder)
        if (cache.get(key) == null) request(src.asset, src.pageIndex.toInt(), id, key, pageWidth, onReady)

        // Zoomed in beyond the whole-page bitmap: a sharp tile of the visible region.
        if (need > pageWidth * 1.25f && visible != null) {
            val region = RectF(visible)
            if (region.intersect(place)) {
                val tw = (region.width() * pxPerUnit).roundToInt()
                val th = (region.height() * pxPerUnit).roundToInt()
                if (tw in 1..MAX_TILE_PX && th in 1..MAX_TILE_PX) {
                    // Snap the region so small pans reuse the tile.
                    val tKey = "$id@tile@${pxPerUnit.roundToInt()}@${(region.left / 64).toInt()}:${(region.top / 64).toInt()}"
                    val current = tile
                    val rect = tileRect
                    if (current != null && current.first.startsWith("$id@tile") && rect != null) {
                        canvas.drawBitmap(current.second, null, rect, paint)
                    }
                    if (current?.first != tKey) {
                        val r = RectF(region.left - 64, region.top - 64, region.right + 64, region.bottom + 64)
                        r.intersect(place)
                        requestTile(src.asset, src.pageIndex.toInt(), tKey, place, r, pxPerUnit, onReady)
                    }
                }
            }
        }
        return true
    }

    private fun request(hash: String, page: Int, id: String, key: String, width: Int, onReady: () -> Unit) {
        if (!pending.add(key)) return
        worker.execute {
            val bmp = runCatching { renderPage(hash, page, width) }.getOrNull()
            main.post {
                pending.remove(key)
                if (bmp != null) {
                    aspects[id] = bmp.width.toFloat() / bmp.height
                    cache.put(key, bmp)
                    if ((best[id] ?: 0) < width) best[id] = width
                    onReady()
                }
            }
        }
    }

    private fun requestTile(hash: String, page: Int, key: String, place: RectF, region: RectF, pxPerUnit: Float, onReady: () -> Unit) {
        if (!pending.add(key)) return
        worker.execute {
            val bmp = runCatching {
                val doc = open(hash) ?: return@runCatching null
                doc.renderer.openPage(page).use { p ->
                    val w = (region.width() * pxPerUnit).roundToInt().coerceAtLeast(1)
                    val h = (region.height() * pxPerUnit).roundToInt().coerceAtLeast(1)
                    val out = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
                    out.eraseColor(Color.WHITE)
                    // PDF points → tile pixels: scale the page to `place` at this zoom, then
                    // shift so the region's corner is at (0, 0).
                    val k = place.width() * pxPerUnit / p.width
                    val m = Matrix().apply {
                        setScale(k, k)
                        postTranslate((place.left - region.left) * pxPerUnit, (place.top - region.top) * pxPerUnit)
                    }
                    p.render(out, null, m, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                    out
                }
            }.getOrNull()
            main.post {
                pending.remove(key)
                if (bmp != null) {
                    tile = key to bmp
                    tileRect = RectF(region)
                    onReady()
                }
            }
        }
    }

    /** Whole page `page` of PDF `hash`, `width` px wide, white under transparent areas. */
    private fun renderPage(hash: String, page: Int, width: Int): Bitmap? {
        val doc = open(hash) ?: return null
        if (page !in 0 until doc.renderer.pageCount) return null
        return doc.renderer.openPage(page).use { p ->
            val h = (width.toFloat() * p.height / p.width).roundToInt().coerceAtLeast(1)
            val bmp = Bitmap.createBitmap(width, h, Bitmap.Config.ARGB_8888)
            bmp.eraseColor(Color.WHITE)
            p.render(bmp, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
            bmp
        }
    }

    /**
     * Synchronous render for exports (PNG, PDF, SVG): the PDF page of `d` at `pxPerUnit`, or
     * null when `d` shows no PDF or the PDF is not on this device. Call off the PDF thread.
     */
    fun renderForExport(d: PageDoc, pxPerUnit: Float): Pair<Bitmap, RectF>? {
        val src = d.pdfSource() ?: return null
        val dims = d.pageDims()
        val width = (dims[0] * pxPerUnit).roundToInt().coerceIn(1, 4096)
        val bmp = worker.submit<Bitmap?> { runCatching { renderPage(src.asset, src.pageIndex.toInt(), width) }.getOrNull() }.get()
            ?: return null
        return bmp to placement(d, bmp.width.toFloat(), bmp.height.toFloat())
    }

    /** Page sizes in points of the PDF at `path`, as `PdfRenderer` sees them. */
    fun pageSizes(path: String): List<Pair<Float, Float>> =
        ParcelFileDescriptor.open(File(path), ParcelFileDescriptor.MODE_READ_ONLY).use { fd ->
            PdfRenderer(fd).use { r -> (0 until r.pageCount).map { i -> r.openPage(i).use { it.width.toFloat() to it.height.toFloat() } } }
        }
}
