package app.kahie

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Handler
import android.os.Looper
import android.widget.ImageView
import uniffi.nb_ffi.NotebookDoc
import uniffi.nb_ffi.PageDoc
import java.io.File
import java.util.concurrent.Executors

/**
 * Page thumbnails for the library (notebook covers) and the page list (P1-02, P1-03).
 *
 * Rendered off the main thread with [PageRenderer.drawPage] (formulas are left out: they need
 * the WebView renderer), kept in memory and as PNG files in the cache directory. A thumbnail
 * is keyed by the page file's size and modification time (and its WAL's), so an edited page
 * is redrawn and an unchanged one comes straight from the cache.
 */
object Thumbnails {
    private val worker = Executors.newFixedThreadPool(2) { r ->
        Thread(r, "kahie-thumbs").apply { isDaemon = true; priority = Thread.MIN_PRIORITY }
    }
    private val main = Handler(Looper.getMainLooper())
    private val memory = object : android.util.LruCache<String, Bitmap>(16 * 1024 * 1024) {
        override fun sizeOf(key: String, value: Bitmap) = value.byteCount
    }

    /** Cache key of a page file: changes whenever the page is written. */
    private fun key(pagePath: String, widthPx: Int): String {
        val f = File(pagePath)
        val wal = File("$pagePath-wal")
        return "$pagePath|${f.length()}|${f.lastModified()}|${wal.length()}|${wal.lastModified()}|$widthPx"
    }

    private fun diskFile(context: Context, key: String): File {
        val digest = java.security.MessageDigest.getInstance("SHA-1").digest(key.toByteArray())
        val name = digest.joinToString("") { "%02x".format(it) }
        return File(File(context.cacheDir, "thumbs").apply { mkdirs() }, "$name.png")
    }

    /**
     * Show the thumbnail of the page stored at `pagePath` (or of the page `open`, when it is
     * the one being edited) in `view`, `widthPx` wide. The view's tag guards against a late
     * result landing in a view that now shows something else.
     */
    fun load(context: Context, view: ImageView, pagePath: String, widthPx: Int, open: PageDoc? = null) {
        val k = key(pagePath, widthPx)
        view.tag = k
        memory.get(k)?.let { view.setImageBitmap(it); return }
        view.setImageDrawable(null)
        val app = context.applicationContext
        worker.execute {
            val bmp = runCatching { render(app, k, pagePath, widthPx, open) }.getOrNull() ?: return@execute
            memory.put(k, bmp)
            main.post { if (view.tag == k) view.setImageBitmap(bmp) }
        }
    }

    /** Show the cover of the notebook in `notebookDir` (its first page) in `view`. */
    fun loadCover(context: Context, view: ImageView, notebookDir: File, widthPx: Int) {
        val tag = "cover|${notebookDir.path}|$widthPx"
        view.tag = tag
        view.setImageDrawable(null)
        val app = context.applicationContext
        worker.execute {
            val bmp = runCatching {
                val nb = NotebookDoc.open(notebookDir.path, "")
                val path = nb.pageIds().firstOrNull()?.let { nb.pagePath(it) }
                if (path == null) null else {
                    val k = key(path, widthPx)
                    memory.get(k) ?: render(app, k, path, widthPx, null)?.also { memory.put(k, it) }
                }
            }.getOrNull() ?: return@execute
            main.post { if (view.tag == tag) view.setImageBitmap(bmp) }
        }
    }

    private fun render(context: Context, key: String, pagePath: String, widthPx: Int, open: PageDoc?): Bitmap? {
        val disk = diskFile(context, key)
        if (disk.isFile) {
            android.graphics.BitmapFactory.decodeFile(disk.path)?.let {
                disk.setLastModified(System.currentTimeMillis())
                return it
            }
        }
        // A page never opened has no file yet: show blank lined paper rather than create it.
        val doc = open ?: if (File(pagePath).isFile) PageDoc.open(pagePath) else null
        val dims = doc?.pageDims() ?: listOf(PageRenderer.PAGE_W, PageRenderer.PAGE_H)
        val scale = widthPx / dims[0]
        val h = (dims[1] * scale).toInt().coerceAtLeast(1)
        val bmp = Bitmap.createBitmap(widthPx, h, Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp)
        c.scale(scale, scale)
        if (doc != null) {
            PageRenderer.drawPage(c, doc, null)
        } else {
            c.drawColor(android.graphics.Color.WHITE)
            PageRenderer.drawPaper(c, "lined", 0.75f, dims[0], dims[1])
        }
        runCatching {
            val tmp = File(disk.path + ".tmp")
            tmp.outputStream().use { bmp.compress(Bitmap.CompressFormat.PNG, 90, it) }
            tmp.renameTo(disk)
        }
        return bmp
    }

    /** Remove cached thumbnails not used for 30 days. */
    fun trimDiskCache(context: Context) {
        val app = context.applicationContext
        worker.execute {
            val old = System.currentTimeMillis() - 30L * 24 * 3600 * 1000
            File(app.cacheDir, "thumbs").listFiles()?.forEach { if (it.lastModified() < old) it.delete() }
        }
    }
}
