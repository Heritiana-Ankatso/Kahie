package app.kahie

import android.graphics.Bitmap
import android.graphics.Canvas
import uniffi.nb_ffi.DocFormat
import uniffi.nb_ffi.NotebookDoc
import uniffi.nb_ffi.PageDoc
import java.io.OutputStream
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

/**
 * Markdown / LaTeX export of a notebook (E2.12) as one ZIP: `notebook.md` or `notebook.tex`
 * plus `assets/`. The core writes the text and lists the pictures; this renders the
 * handwriting drawings and (for Markdown) the plots as PNG at 2× (192 dpi), and inserted
 * SVG pictures as one-page vector PDFs for LaTeX (Markdown gets the SVG file itself).
 */
object DocExport {
    private const val SCALE = 2f

    fun write(notebook: NotebookDoc, format: DocFormat, page: (String) -> PageDoc, out: OutputStream) {
        val doc = notebook.exportDocument(format)
        ZipOutputStream(out).use { zip ->
            zip.putNextEntry(ZipEntry(if (format == DocFormat.LATEX) "notebook.tex" else "notebook.md"))
            zip.write(doc.text.toByteArray())
            zip.closeEntry()
            val pages = HashMap<String, PageDoc>()
            for (img in doc.images) {
                val bytes = img.bytes ?: run {
                    val p = pages.getOrPut(img.pageId) { page(img.pageId) }
                    if (img.kind == "svg") return@run svgPdf(p, img.blockId)
                    val bmp = when (img.kind) {
                        "drawing" -> drawing(p, img.crop)
                        "plot" -> p.plotBlocks().firstOrNull { it.id == img.blockId }?.let { plot(it) }
                        else -> null
                    } ?: return@run null
                    val buf = java.io.ByteArrayOutputStream()
                    bmp.compress(Bitmap.CompressFormat.PNG, 100, buf)
                    bmp.recycle()
                    buf.toByteArray()
                } ?: continue
                zip.putNextEntry(ZipEntry(img.path))
                zip.write(bytes)
                zip.closeEntry()
            }
        }
    }

    /** An inserted SVG picture as a one-page vector PDF the size of its box (LaTeX export). */
    private fun svgPdf(p: PageDoc, blockId: String): ByteArray? {
        val im = p.imageBlocks().firstOrNull { it.id == blockId } ?: return null
        val pic = PageRenderer.svgPicture(p, im.hash) ?: return null
        return SvgImages.pdf(pic, im.width, im.height)
    }

    /** The page's background and ink inside `crop` (x, y, w, h), transparent elsewhere. */
    private fun drawing(d: PageDoc, crop: List<Float>): Bitmap? {
        if (crop.size < 4 || crop[2] <= 0f || crop[3] <= 0f) return null
        val bmp = Bitmap.createBitmap((crop[2] * SCALE).toInt().coerceAtLeast(1), (crop[3] * SCALE).toInt().coerceAtLeast(1), Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp)
        c.scale(SCALE, SCALE)
        c.translate(-crop[0], -crop[1])
        PageRenderer.drawBackground(c, d)
        for (key in d.layout()) {
            val g = d.geometry(key) ?: continue
            c.drawPath(PageRenderer.inkPath(g), PageRenderer.inkPaint(g))
        }
        return bmp
    }

    private fun plot(p: uniffi.nb_ffi.PlotBlock): Bitmap {
        val bmp = Bitmap.createBitmap((p.width * SCALE).toInt().coerceAtLeast(1), (p.height * SCALE).toInt().coerceAtLeast(1), Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp)
        c.scale(SCALE, SCALE)
        c.translate(-p.x, -p.y)
        PageRenderer.drawPlot(c, p, 0.75f)
        return bmp
    }
}
