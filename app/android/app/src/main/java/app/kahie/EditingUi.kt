package app.kahie

import android.app.Activity
import android.app.AlertDialog
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.widget.CheckBox
import android.widget.GridLayout
import android.widget.LinearLayout
import android.widget.RadioButton
import android.widget.RadioGroup
import android.widget.TextView
import android.widget.Toast
import uniffi.nb_ffi.AttachmentBlock
import uniffi.nb_ffi.PageDoc
import java.io.File
import java.util.concurrent.Executor

/** Attached files (P1-19): drawn as a labelled card on the page and in exports. */
object Attachments {
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFF1F4FA.toInt() }
    private val border = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; color = 0xFF8EA2C8.toInt() }
    private val label = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF1D1D1F.toInt() }
    private val sub = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF6B6F78.toInt() }

    /** Short type label for a file ("PDF", "DOCX"…). */
    fun kind(a: AttachmentBlock): String =
        a.name.substringAfterLast('.', "").takeIf { it.isNotEmpty() && it.length <= 5 }?.uppercase()
            ?: a.mime.substringAfter('/').take(8).uppercase()

    /** `unit` is one screen pixel in page units (for hairlines). */
    fun draw(canvas: Canvas, a: AttachmentBlock, unit: Float) {
        val r = RectF(a.x, a.y, a.x + a.width, a.y + a.height)
        val corner = minOf(8f, a.height / 4)
        canvas.drawRoundRect(r, corner, corner, fill)
        border.strokeWidth = unit
        canvas.drawRoundRect(r, corner, corner, border)
        // A sheet with a folded corner, then the name and type.
        val h = a.height * 0.6f
        val w = h * 0.75f
        val x = a.x + a.height * 0.2f
        val y = a.y + a.height * 0.2f
        val sheet = android.graphics.Path().apply {
            moveTo(x, y); lineTo(x + w * 0.7f, y); lineTo(x + w, y + w * 0.3f); lineTo(x + w, y + h); lineTo(x, y + h); close()
        }
        canvas.drawPath(sheet, border)
        label.textSize = a.height * 0.3f
        sub.textSize = a.height * 0.22f
        val tx = x + w + a.height * 0.2f
        val avail = a.x + a.width - tx - 4f
        val n = label.breakText(a.name, true, avail.coerceAtLeast(1f), null)
        val name = if (n < a.name.length) a.name.take((n - 1).coerceAtLeast(1)) + "…" else a.name
        canvas.drawText(name, tx, a.y + a.height * 0.45f, label)
        canvas.drawText(kind(a), tx, a.y + a.height * 0.8f, sub)
    }
}

/**
 * Phase 1 editing UI (P1-06…P1-19) kept out of [MainActivity]: tool options (pen, pencil,
 * brush, highlighter; 12 colours and a custom one; 5 widths; eraser and stylus button
 * mapping), the Insert menu (camera, clipboard, attachments), system clipboard, groups and
 * image crop/rotate.
 */
class EditingUi(
    private val activity: Activity,
    private val ink: InkView,
    private val doc: () -> PageDoc?,
    private val io: Executor,
) {
    companion object {
        const val REQ_CAMERA = 61
        const val REQ_ATTACH = 62
        const val REQ_PICK_IMAGE = 63
        /** Label of clips Kahie puts on the system clipboard (so Paste prefers its own copy). */
        const val CLIP_LABEL = "Kahie"
        /** Largest attachment stored in a page (they sync with it). */
        const val MAX_ATTACHMENT = 25 * 1024 * 1024
        /** Largest TIFF file read for insertion (scans can be big; decoded and downsized by the core). */
        const val MAX_TIFF = 200 * 1024 * 1024

        val penPalette = intArrayOf(
            0xFF1D1D1F.toInt(), 0xFF6B6F78.toInt(), 0xFF2B59C3.toInt(), 0xFF4FA3E0.toInt(),
            0xFFD1495B.toInt(), 0xFFE67E22.toInt(), 0xFFD4A017.toInt(), 0xFF2A9D8F.toInt(),
            0xFF5BAA3C.toInt(), 0xFF8E44AD.toInt(), 0xFFE06AA3.toInt(), 0xFF8B5A2B.toInt(),
        )
        val highlighterPalette = intArrayOf(
            0xFFFFE066.toInt(), 0xFF9BE564.toInt(), 0xFF7CC6FE.toInt(), 0xFFFF99C8.toInt(),
            0xFFFFB86B.toInt(), 0xFFC9A7FF.toInt(), 0xFF7FE0D0.toInt(), 0xFFFFD1A1.toInt(),
            0xFFD0D0D0.toInt(), 0xFFB5E48C.toInt(), 0xFFFFADAD.toInt(), 0xFFA0C4FF.toInt(),
        )
        /** Width presets, as factors of the default pen (3), highlighter (20) and eraser (9). */
        val widths = doubleArrayOf(0.4, 0.6, 1.0, 1.6, 2.5)

        /** A grid of `colors`, plus "Custom…" (hex) and, with `allowDefault`, "Default" (null). */
        fun pickColor(activity: Activity, colors: IntArray, title: Int, allowDefault: Boolean = true, onPick: (Int?) -> Unit) {
            val dp = activity.resources.displayMetrics.density
            var dialog: AlertDialog? = null
            val grid = GridLayout(activity).apply { columnCount = 6; setPadding((12 * dp).toInt(), (8 * dp).toInt(), (12 * dp).toInt(), 0) }
            for ((i, c) in colors.withIndex()) {
                grid.addView(View(activity).apply {
                    background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(c); setStroke((1 * dp).toInt(), 0xFFDEDAD0.toInt()) }
                    contentDescription = activity.getString(R.string.color) + " " + (i + 1)
                    layoutParams = GridLayout.LayoutParams().apply {
                        width = (36 * dp).toInt(); height = (36 * dp).toInt(); setMargins((5 * dp).toInt(), (5 * dp).toInt(), (5 * dp).toInt(), (5 * dp).toInt())
                    }
                    setOnClickListener { onPick(c); dialog?.dismiss() }
                })
            }
            val b = AlertDialog.Builder(activity)
                .setTitle(title)
                .setView(grid)
                .setNeutralButton(R.string.custom_color) { _, _ -> customColor(activity, onPick) }
                .setNegativeButton(android.R.string.cancel, null)
            if (allowDefault) b.setPositiveButton(R.string.default_color) { _, _ -> onPick(null) }
            dialog = b.show()
        }

        private fun customColor(activity: Activity, onPick: (Int?) -> Unit) {
            val input = android.widget.EditText(activity).apply { hint = "#2B59C3"; setSingleLine() }
            AlertDialog.Builder(activity)
                .setTitle(R.string.custom_color)
                .setView(input)
                .setNegativeButton(android.R.string.cancel, null)
                .setPositiveButton(android.R.string.ok) { _, _ ->
                    val hex = input.text.toString().trim().removePrefix("#")
                    val v = if (hex.length == 6) hex.toIntOrNull(16) else null
                    if (v == null) Toast.makeText(activity, R.string.custom_color_bad, Toast.LENGTH_SHORT).show()
                    else onPick(v or (0xFF shl 24))
                }
                .show()
        }
    }

    private val main = Handler(Looper.getMainLooper())
    private val prefs = activity.getSharedPreferences("kahie", Activity.MODE_PRIVATE)
    private val dp = activity.resources.displayMetrics.density

    private fun toast(msg: String) = Toast.makeText(activity, msg, Toast.LENGTH_SHORT).show()
    private fun changed() { ink.refresh(); ink.onChanged() }

    /** Apply the saved tool settings to the canvas. */
    fun restore() {
        ink.partialEraser = prefs.getBoolean("partialEraser", false)
        ink.eraserEndPartial = prefs.getBoolean("eraserEndPartial", false)
        ink.sideButton = prefs.getInt("sideButton", 0)
        setWidth(prefs.getInt("widthPreset", 2))
    }

    fun setWidth(i: Int) {
        val f = widths[i.coerceIn(0, widths.size - 1)]
        ink.penWidth = 3.0 * f
        ink.highlighterWidth = 20.0 * f
        ink.eraserRadius = 9.0 * f
        prefs.edit().putInt("widthPreset", i).apply()
    }

    // ---------- tool options (P1-10, P1-11) ----------

    /** Tool, colour, width, eraser mode and stylus button mapping. `onDone` refreshes the toolbar. */
    fun showToolOptions(onDone: () -> Unit) {
        val box = LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            val pad = (16 * dp).toInt()
            setPadding(pad, pad / 2, pad, 0)
        }
        fun heading(res: Int) = TextView(activity).apply {
            text = activity.getString(res); textSize = 13f; setTextColor(0xFF6B6F78.toInt()); setPadding(0, (10 * dp).toInt(), 0, (4 * dp).toInt())
        }.also { box.addView(it) }
        heading(R.string.tool)
        val tools = listOf(
            InkView.Tool.PEN to R.string.pen, InkView.Tool.PENCIL to R.string.pencil,
            InkView.Tool.BRUSH to R.string.brush, InkView.Tool.HIGHLIGHTER to R.string.highlighter,
        )
        val group = RadioGroup(activity).apply { orientation = RadioGroup.HORIZONTAL }
        for ((i, t) in tools.withIndex()) {
            group.addView(RadioButton(activity).apply {
                id = 1000 + i
                text = activity.getString(t.second)
                isChecked = ink.tool == t.first
            })
        }
        group.setOnCheckedChangeListener { _, id -> tools.getOrNull(id - 1000)?.let { ink.clearSelection(); ink.tool = it.first; onDone() } }
        box.addView(group)

        heading(R.string.color)
        val swatches = LinearLayout(activity).apply { orientation = LinearLayout.HORIZONTAL }
        fun refreshSwatch(v: View) {
            val hl = ink.tool == InkView.Tool.HIGHLIGHTER
            (v.background as GradientDrawable).setColor(if (hl) ink.highlighterColor else ink.penColor)
        }
        val current = View(activity).apply {
            background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setStroke((1 * dp).toInt(), 0xFFDEDAD0.toInt()) }
            layoutParams = LinearLayout.LayoutParams((36 * dp).toInt(), (36 * dp).toInt())
            contentDescription = activity.getString(R.string.color)
        }
        refreshSwatch(current)
        swatches.addView(current)
        swatches.addView(TextView(activity).apply {
            text = activity.getString(R.string.choose_color)
            setPadding((12 * dp).toInt(), 0, 0, 0)
            gravity = Gravity.CENTER_VERTICAL
            minHeight = (36 * dp).toInt()
            setTextColor(0xFF2B59C3.toInt())
            setOnClickListener {
                val hl = ink.tool == InkView.Tool.HIGHLIGHTER
                pickColor(activity, if (hl) highlighterPalette else penPalette, R.string.color, allowDefault = false) { c ->
                    if (c == null) return@pickColor
                    if (hl) ink.highlighterColor = c else ink.penColor = c
                    refreshSwatch(current)
                    onDone()
                }
            }
        })
        box.addView(swatches)

        heading(R.string.width)
        val widthRow = LinearLayout(activity).apply { orientation = LinearLayout.HORIZONTAL }
        val selected = prefs.getInt("widthPreset", 2)
        val dots = ArrayList<View>()
        for ((i, f) in widths.withIndex()) {
            val dot = (4 + f * 7) * dp
            val v = object : View(activity) {
                val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF1D1D1F.toInt() }
                override fun onDraw(c: Canvas) {
                    if (isSelected) c.drawCircle(width / 2f, height / 2f, width / 2f - 2 * dp, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0x332B59C3 })
                    c.drawCircle(width / 2f, height / 2f, dot.toFloat() / 2, paint)
                }
            }
            v.isSelected = i == selected
            v.contentDescription = activity.getString(R.string.width) + " " + (i + 1)
            v.layoutParams = LinearLayout.LayoutParams((44 * dp).toInt(), (44 * dp).toInt())
            v.setOnClickListener { setWidth(i); dots.forEachIndexed { k, d -> d.isSelected = k == i; d.invalidate() } }
            dots.add(v)
            widthRow.addView(v)
        }
        box.addView(widthRow)

        heading(R.string.eraser)
        box.addView(CheckBox(activity).apply {
            text = activity.getString(R.string.partial_eraser)
            isChecked = ink.partialEraser
            setOnCheckedChangeListener { _, on -> ink.partialEraser = on; prefs.edit().putBoolean("partialEraser", on).apply() }
        })
        box.addView(CheckBox(activity).apply {
            text = activity.getString(R.string.eraser_end_partial)
            isChecked = ink.eraserEndPartial
            setOnCheckedChangeListener { _, on -> ink.eraserEndPartial = on; prefs.edit().putBoolean("eraserEndPartial", on).apply() }
        })
        heading(R.string.side_button)
        val side = RadioGroup(activity).apply { orientation = RadioGroup.VERTICAL }
        for ((i, res) in listOf(R.string.side_button_erase, R.string.side_button_partial, R.string.side_button_none).withIndex()) {
            side.addView(RadioButton(activity).apply { id = 2000 + i; text = activity.getString(res); isChecked = ink.sideButton == i })
        }
        side.setOnCheckedChangeListener { _, id -> ink.sideButton = id - 2000; prefs.edit().putInt("sideButton", id - 2000).apply() }
        box.addView(side)

        AlertDialog.Builder(activity)
            .setTitle(R.string.tool_options)
            .setView(android.widget.ScrollView(activity).apply { addView(box) })
            .setPositiveButton(android.R.string.ok) { _, _ -> onDone() }
            .show()
    }

    // ---------- insert (P1-18, P1-19) ----------

    fun showInsertMenu() {
        val items = arrayOf(
            activity.getString(R.string.insert_photo_gallery), activity.getString(R.string.insert_camera),
            activity.getString(R.string.insert_clipboard), activity.getString(R.string.insert_file),
        )
        AlertDialog.Builder(activity)
            .setTitle(R.string.insert)
            .setItems(items) { _, which ->
                when (which) {
                    0 -> activity.startActivityForResult(
                        Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("image/*"), REQ_PICK_IMAGE,
                    )
                    1 -> takePhoto()
                    2 -> if (!pasteFromSystem(forceSystem = true)) toast(activity.getString(R.string.clipboard_empty))
                    else -> activity.startActivityForResult(
                        Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("*/*"), REQ_ATTACH,
                    )
                }
            }
            .show()
    }

    private fun takePhoto() {
        val f = File(ShareProvider.dir(activity), "capture-${System.currentTimeMillis()}.jpg")
        runCatching { f.createNewFile() }
        prefs.edit().putString("captureFile", f.path).apply()
        val uri = ShareProvider.uriFor(f)
        val intent = Intent(android.provider.MediaStore.ACTION_IMAGE_CAPTURE)
            .putExtra(android.provider.MediaStore.EXTRA_OUTPUT, uri)
            .addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION or Intent.FLAG_GRANT_READ_URI_PERMISSION)
        intent.clipData = ClipData.newRawUri("", uri)
        try {
            activity.startActivityForResult(intent, REQ_CAMERA)
        } catch (e: android.content.ActivityNotFoundException) {
            toast(activity.getString(R.string.no_camera))
        }
    }

    /** Handles this class's activity results; returns false for other request codes. */
    fun onResult(requestCode: Int, resultCode: Int, data: Intent?): Boolean {
        when (requestCode) {
            REQ_CAMERA -> {
                val f = prefs.getString("captureFile", null)?.let { File(it) }
                if (resultCode == Activity.RESULT_OK && f != null && f.length() > 0) {
                    insertPicture({ f.delete() }) { raster(android.graphics.ImageDecoder.createSource(f)) }
                } else {
                    f?.delete()
                }
            }
            REQ_PICK_IMAGE -> data?.data?.takeIf { resultCode == Activity.RESULT_OK }?.let { insertImage(it) }
            REQ_ATTACH -> data?.data?.takeIf { resultCode == Activity.RESULT_OK }?.let { attach(it) }
            else -> return false
        }
        return true
    }

    /** A picture ready for the page: the bytes to store and its intrinsic size (for the box aspect). */
    private class Prepared(val bytes: ByteArray, val width: Float, val height: Float)

    /** The picked or pasted file is not a picture Kahie can read. */
    private class UnsupportedImage : Exception()

    /** The file is larger than Kahie reads for a picture of its kind. */
    private class TooLarge(val megabytes: Int) : Exception()

    /** Insert the picture at `uri` (gallery, file picker, clipboard): raster formats, SVG/SVGZ, TIFF. */
    fun insertImage(uri: Uri) = insertPicture { prepare(uri) }

    /**
     * Prepare and place a picture at the centre of the view: a box `min(420, page width − 40)`
     * wide with the picture's aspect.
     */
    private fun insertPicture(after: () -> Unit = {}, prepare: () -> Prepared) {
        val d = doc() ?: return
        val centre = ink.visibleCentre()
        io.execute {
            val result = runCatching(prepare)
            main.post {
                after()
                result.onSuccess { pic ->
                    val boxW = minOf(420f, if (d.isInfinite()) 420f else d.pageDims()[0] - 40f)
                    val boxH = boxW * pic.height / pic.width
                    d.addImage((centre[0] - boxW / 2).toDouble(), (centre[1] - boxH / 2).toDouble(), boxW.toDouble(), boxH.toDouble(), pic.bytes)
                    changed()
                }.onFailure { e ->
                    val msg = when (e) {
                        is UnsupportedImage, is android.graphics.ImageDecoder.DecodeException, is uniffi.nb_ffi.CoreException ->
                            activity.getString(R.string.image_unsupported)
                        is TooLarge -> activity.getString(R.string.attachment_too_large, e.megabytes)
                        else -> activity.getString(R.string.transfer_failed, e.message ?: "")
                    }
                    toast(msg)
                }
            }
        }
    }

    /**
     * Read the picture at `uri`. SVG (and gzipped SVGZ) is kept as its SVG text, so it stays
     * vector; formats Android decodes (JPEG, PNG, WebP, GIF's first frame, HEIF/HEIC, AVIF,
     * BMP, ICO, DNG) are downsized to ≤ 1600 px and stored as PNG (with transparency) or
     * JPEG; TIFF is decoded by the core and stored as PNG.
     */
    private fun prepare(uri: Uri): Prepared {
        val resolver = activity.contentResolver
        val mime = resolver.getType(uri)?.lowercase()
        val head = resolver.openInputStream(uri)?.use { readUpTo(it, 4096) } ?: throw java.io.FileNotFoundException(uri.toString())
        if (mime == "image/svg+xml" || SvgImages.isSvg(head) || SvgImages.isGzip(head)) {
            val bytes = resolver.openInputStream(uri)!!.use { readUpTo(it, SvgImages.MAX_BYTES + 1) }
            svg(bytes)?.let { return it }
            if (mime == "image/svg+xml") throw UnsupportedImage()
        }
        val raster = runCatching { raster(android.graphics.ImageDecoder.createSource(resolver, uri)) }
        raster.getOrNull()?.let { return it }
        if (uniffi.nb_ffi.isTiff(head)) {
            val bytes = resolver.openInputStream(uri)!!.use { readUpTo(it, MAX_TIFF + 1) }
            if (bytes.size > MAX_TIFF) throw TooLarge(MAX_TIFF / (1024 * 1024))
            val t = uniffi.nb_ffi.decodeTiff(bytes)
            return Prepared(t.png, t.width.toFloat(), t.height.toFloat())
        }
        throw raster.exceptionOrNull()?.takeIf { it !is android.graphics.ImageDecoder.DecodeException } ?: UnsupportedImage()
    }

    /** An SVG picture from SVG or SVGZ bytes (checked to parse), or null if they are not SVG. */
    private fun svg(bytes: ByteArray): Prepared? {
        if (bytes.size > SvgImages.MAX_BYTES) throw TooLarge(SvgImages.MAX_BYTES / (1024 * 1024))
        val text = SvgImages.normalise(bytes) ?: return null
        val parsed = runCatching { SvgImages.parse(text) }.getOrNull() ?: throw UnsupportedImage()
        val (w, h) = SvgImages.size(parsed)
        if (!(w > 0f && h > 0f) || w.isInfinite() || h.isInfinite()) throw UnsupportedImage()
        return Prepared(text, w, h)
    }

    private fun raster(src: android.graphics.ImageDecoder.Source): Prepared {
        val (bytes, w, h) = encode(decode(src))
        return Prepared(bytes, w.toFloat(), h.toFloat())
    }

    private fun readUpTo(inp: java.io.InputStream, max: Int): ByteArray {
        val buf = java.io.ByteArrayOutputStream()
        val chunk = ByteArray(64 * 1024)
        while (buf.size() < max) {
            val n = inp.read(chunk, 0, minOf(chunk.size, max - buf.size()))
            if (n < 0) break
            buf.write(chunk, 0, n)
        }
        return buf.toByteArray()
    }

    private fun decode(src: android.graphics.ImageDecoder.Source): Bitmap =
        android.graphics.ImageDecoder.decodeBitmap(src) { decoder, info, _ ->
            val w = info.size.width
            val h = info.size.height
            val scale = minOf(1f, 1600f / maxOf(w, h))
            decoder.setTargetSize(maxOf(1, (w * scale).toInt()), maxOf(1, (h * scale).toInt()))
            decoder.allocator = android.graphics.ImageDecoder.ALLOCATOR_SOFTWARE
        }

    private fun encode(bmp: Bitmap): Triple<ByteArray, Int, Int> {
        val out = java.io.ByteArrayOutputStream()
        val png = bmp.hasAlpha()
        bmp.compress(if (png) Bitmap.CompressFormat.PNG else Bitmap.CompressFormat.JPEG, 85, out)
        return Triple(out.toByteArray(), bmp.width, bmp.height)
    }

    private fun displayName(uri: Uri): String? = runCatching {
        activity.contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            if (c.moveToFirst()) c.getString(0) else null
        }
    }.getOrNull()

    /** Store any file in the page (P1-19). */
    private fun attach(uri: Uri) {
        val d = doc() ?: return
        val centre = ink.visibleCentre()
        val name = displayName(uri) ?: uri.lastPathSegment ?: "file"
        val mime = activity.contentResolver.getType(uri) ?: "application/octet-stream"
        io.execute {
            val result = runCatching {
                activity.contentResolver.openInputStream(uri)!!.use { inp ->
                    val buf = java.io.ByteArrayOutputStream()
                    val chunk = ByteArray(64 * 1024)
                    while (true) {
                        val n = inp.read(chunk)
                        if (n < 0) break
                        buf.write(chunk, 0, n)
                        if (buf.size() > MAX_ATTACHMENT) throw IllegalStateException(activity.getString(R.string.attachment_too_large, MAX_ATTACHMENT / (1024 * 1024)))
                    }
                    buf.toByteArray()
                }
            }
            main.post {
                result.onSuccess { bytes ->
                    d.addAttachment((centre[0] - 110f).toDouble(), (centre[1] - 28f).toDouble(), 220.0, 56.0, bytes, name, mime)
                    changed()
                }.onFailure { toast(activity.getString(R.string.transfer_failed, it.message ?: "")) }
            }
        }
    }

    /** Open an attached file with another app (a read-only copy in the share folder). */
    fun openAttachment(id: String) {
        val d = doc() ?: return
        val a = d.attachmentBlocks().firstOrNull { it.id == id } ?: return
        io.execute {
            val result = runCatching {
                val bytes = d.imageBytes(a.hash) ?: throw java.io.FileNotFoundException(a.name)
                val safe = a.name.replace(Regex("[^A-Za-z0-9._ -]"), "_").take(80).ifBlank { "file" }
                val f = File(ShareProvider.dir(activity), "att-${a.hash.take(8)}-$safe")
                f.writeBytes(bytes)
                f
            }
            main.post {
                result.onSuccess { f ->
                    val view = Intent(Intent.ACTION_VIEW)
                        .setDataAndType(ShareProvider.uriFor(f), a.mime)
                        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    try {
                        activity.startActivity(Intent.createChooser(view, a.name))
                    } catch (e: android.content.ActivityNotFoundException) {
                        toast(activity.getString(R.string.no_app_for_file))
                    }
                }.onFailure { toast(activity.getString(R.string.transfer_failed, it.message ?: "")) }
            }
        }
    }

    // ---------- system clipboard (P1-15) ----------

    private val clipboard: ClipboardManager get() = activity.getSystemService(ClipboardManager::class.java)!!

    /** Put the selection on the system clipboard as well: its text, or a lone image. */
    fun copyToSystem(ids: List<String>) {
        val d = doc() ?: return
        val text = d.selectionText(ids)
        val images = d.imageBlocks().filter { it.id in ids }
        val clip = when {
            text.isNotBlank() -> ClipData.newPlainText(CLIP_LABEL, text)
            images.size == 1 -> d.imageBytes(images[0].hash)?.let { bytes ->
                val ext = when {
                    SvgImages.isSvg(bytes) -> "svg"
                    bytes.size > 3 && bytes[1] == 'P'.code.toByte() -> "png"
                    else -> "jpg"
                }
                val f = File(ShareProvider.dir(activity), "clip-${images[0].hash.take(16)}.$ext")
                f.writeBytes(bytes)
                ClipData.newUri(activity.contentResolver, CLIP_LABEL, ShareProvider.uriFor(f))
            }
            else -> null
        } ?: ClipData.newPlainText(CLIP_LABEL, "")
        runCatching { clipboard.setPrimaryClip(clip) }
    }

    /** Whether the system clipboard holds something copied in another app. */
    fun systemClipAvailable(): Boolean {
        val c = runCatching { clipboard.primaryClip }.getOrNull() ?: return false
        return c.description.label != CLIP_LABEL && c.itemCount > 0
    }

    /**
     * Paste from the system clipboard when it holds something copied outside Kahie (or
     * always with `forceSystem`): text becomes a text block, an image an image. Returns false
     * when there was nothing to paste from it.
     */
    fun pasteFromSystem(forceSystem: Boolean = false): Boolean {
        val c = runCatching { clipboard.primaryClip }.getOrNull() ?: return false
        if (c.itemCount == 0 || (!forceSystem && c.description.label == CLIP_LABEL)) return false
        val item = c.getItemAt(0)
        val uri = item.uri
        if (uri != null && (activity.contentResolver.getType(uri)?.startsWith("image/") == true || c.description.hasMimeType("image/*"))) {
            insertImage(uri)
            return true
        }
        val text = item.coerceToText(activity)?.toString()?.trim().orEmpty()
        if (text.isEmpty()) return false
        val d = doc() ?: return false
        val centre = ink.visibleCentre()
        val width = 420f
        val size = 20f
        val h = ink.measureText(text, width, size)
        d.addText((centre[0] - width / 2).toDouble(), centre[1].toDouble(), width.toDouble(), h.toDouble(), text, size.toDouble(), (ink.penColor and 0xFFFFFF).toUInt())
        changed()
        return true
    }

    // ---------- selection extras (P1-16, P1-18) ----------

    /** Extra entries of the selection menu for `ids`, with their actions. */
    fun selectionExtras(ids: List<String>): List<Pair<String, () -> Unit>> {
        val d = doc() ?: return emptyList()
        val out = ArrayList<Pair<String, () -> Unit>>()
        if (ids.size >= 2 && !d.grouped(ids)) out.add(activity.getString(R.string.group) to { d.groupItems(ids); changed() })
        if (d.grouped(ids)) out.add(activity.getString(R.string.ungroup) to { d.ungroupItems(ids); changed() })
        val img = d.imageBlocks().filter { it.id in ids }
        if (ids.size == 1 && img.size == 1) out.add(activity.getString(R.string.crop_rotate) to { cropRotate(img[0]) })
        val att = d.attachmentBlocks().filter { it.id in ids }
        if (ids.size == 1 && att.size == 1) out.add(activity.getString(R.string.open_attachment) to { openAttachment(att[0].id) })
        return out
    }

    /** Crop and rotate an image (P1-18); the new picture replaces it in one undo step. */
    private fun cropRotate(img: uniffi.nb_ffi.ImageBlock) {
        val d = doc() ?: return
        val original = PageRenderer.bitmap(d, img.hash) ?: return
        val view = CropView(activity, original)
        val rotate = TextView(activity).apply {
            text = activity.getString(R.string.rotate_90)
            setTextColor(0xFF2B59C3.toInt())
            textSize = 15f
            gravity = Gravity.CENTER
            minHeight = (44 * dp).toInt()
            setOnClickListener { view.rotate() }
        }
        val box = LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            addView(view, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, (360 * dp).toInt()))
            addView(rotate)
        }
        AlertDialog.Builder(activity)
            .setTitle(R.string.crop_rotate)
            .setView(box)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val (bmp, frac) = view.result() ?: return@setPositiveButton
                io.execute {
                    val enc = runCatching { encode(bmp) }
                    main.post {
                        enc.onSuccess { (bytes, w, h) ->
                            // Keep the scale of the picture on the page: the kept part of the
                            // width (after rotation) sets the new box width.
                            val oldW = if (view.quarterTurns % 2 == 0) img.width else img.height
                            val boxW = (oldW * frac).coerceAtLeast(8f)
                            d.replaceImage(img.id, bytes, boxW.toDouble(), (boxW * h / w).toDouble())
                            changed()
                            ink.select(listOf(img.id))
                        }.onFailure { toast(activity.getString(R.string.transfer_failed, it.message ?: "")) }
                    }
                }
            }
            .show()
    }

    /** Shows a picture with a crop frame (drag its corners) and rotates by quarter turns. */
    private class CropView(context: android.content.Context, private val source: Bitmap) : View(context) {
        var quarterTurns = 0
            private set
        private var bmp: Bitmap = source
        /** Crop in fractions of the (rotated) picture. */
        private val crop = RectF(0f, 0f, 1f, 1f)
        private val shown = RectF()
        private var dragging = -1
        private val shade = Paint().apply { color = 0x88000000.toInt() }
        private val frame = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; color = Color.WHITE; strokeWidth = 2 * resources.displayMetrics.density }
        private val knob = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE }

        fun rotate() {
            quarterTurns = (quarterTurns + 1) % 4
            bmp = Bitmap.createBitmap(source, 0, 0, source.width, source.height, Matrix().apply { postRotate(90f * quarterTurns) }, true)
            crop.set(0f, 0f, 1f, 1f)
            invalidate()
        }

        /** Cropped picture and the kept fraction of its width, or null if nothing changed. */
        fun result(): Pair<Bitmap, Float>? {
            if (quarterTurns == 0 && crop.left <= 0f && crop.top <= 0f && crop.right >= 1f && crop.bottom >= 1f) return null
            val x = (crop.left * bmp.width).toInt().coerceIn(0, bmp.width - 1)
            val y = (crop.top * bmp.height).toInt().coerceIn(0, bmp.height - 1)
            val w = (crop.width() * bmp.width).toInt().coerceIn(1, bmp.width - x)
            val h = (crop.height() * bmp.height).toInt().coerceIn(1, bmp.height - y)
            return Bitmap.createBitmap(bmp, x, y, w, h) to crop.width()
        }

        override fun onDraw(c: Canvas) {
            val sc = minOf(width.toFloat() / bmp.width, height.toFloat() / bmp.height)
            val w = bmp.width * sc
            val h = bmp.height * sc
            shown.set((width - w) / 2, (height - h) / 2, (width + w) / 2, (height + h) / 2)
            c.drawBitmap(bmp, null, shown, null)
            val r = RectF(shown.left + crop.left * w, shown.top + crop.top * h, shown.left + crop.right * w, shown.top + crop.bottom * h)
            c.drawRect(shown.left, shown.top, shown.right, r.top, shade)
            c.drawRect(shown.left, r.bottom, shown.right, shown.bottom, shade)
            c.drawRect(shown.left, r.top, r.left, r.bottom, shade)
            c.drawRect(r.right, r.top, shown.right, r.bottom, shade)
            c.drawRect(r, frame)
            val k = 8 * resources.displayMetrics.density
            for ((x, y) in corners(r)) c.drawCircle(x, y, k, knob)
        }

        private fun corners(r: RectF) = listOf(r.left to r.top, r.right to r.top, r.right to r.bottom, r.left to r.bottom)

        override fun onTouchEvent(e: MotionEvent): Boolean {
            if (shown.width() <= 0f) return false
            val fx = ((e.x - shown.left) / shown.width()).coerceIn(0f, 1f)
            val fy = ((e.y - shown.top) / shown.height()).coerceIn(0f, 1f)
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    val r = RectF(shown.left + crop.left * shown.width(), shown.top + crop.top * shown.height(), shown.left + crop.right * shown.width(), shown.top + crop.bottom * shown.height())
                    dragging = corners(r).indexOfFirst { (x, y) -> kotlin.math.hypot(x - e.x, y - e.y) < 32 * resources.displayMetrics.density }
                    parent?.requestDisallowInterceptTouchEvent(true)
                }
                MotionEvent.ACTION_MOVE -> {
                    val min = 0.05f
                    when (dragging) {
                        0 -> { crop.left = minOf(fx, crop.right - min); crop.top = minOf(fy, crop.bottom - min) }
                        1 -> { crop.right = maxOf(fx, crop.left + min); crop.top = minOf(fy, crop.bottom - min) }
                        2 -> { crop.right = maxOf(fx, crop.left + min); crop.bottom = maxOf(fy, crop.top + min) }
                        3 -> { crop.left = minOf(fx, crop.right - min); crop.bottom = maxOf(fy, crop.top + min) }
                    }
                    invalidate()
                }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> dragging = -1
            }
            return true
        }
    }
}
