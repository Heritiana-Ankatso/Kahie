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
import uniffi.nb_ffi.NotebookDoc
import uniffi.nb_ffi.OutlineKind
import uniffi.nb_ffi.SyncListener
import uniffi.nb_ffi.SyncSession
import uniffi.nb_ffi.SyncStatus
import uniffi.nb_ffi.casAvailable
import uniffi.nb_ffi.casEval
import uniffi.nb_ffi.exportKahie
import uniffi.nb_ffi.importKahie
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

    /** One directory per notebook under `notebooks/`. */
    private val notebooksRoot by lazy { File(filesDir, "notebooks").apply { mkdirs() } }
    private lateinit var notebook: NotebookDoc
    private var mathRenderer: MathRenderer? = null
    private var pageIds: List<String> = emptyList()
    private var pageIndex = 0
    private var doc: PageDoc? = null

    private val penColors = intArrayOf(0xFF1D1D1F.toInt(), 0xFF2B59C3.toInt(), 0xFFD1495B.toInt(), 0xFF2A9D8F.toInt(), 0xFFE09F3E.toInt())
    private val highlighterColors = intArrayOf(0xFFFFE066.toInt(), 0xFF9BE564.toInt(), 0xFF7CC6FE.toInt(), 0xFFFF99C8.toInt(), 0xFFFFB86B.toInt())
    private val templates = listOf("lined", "grid", "dots", "blank")

    private val dp get() = resources.displayMetrics.density

    private companion object {
        const val REQ_EXPORT = 41
        const val REQ_IMPORT = 42
        const val REQ_EXPORT_PDF = 43
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ink = InkView(this)
        ink.autoShapes = prefs.getBoolean("autoShapes", false)
        ink.onTextTap = { x, y, blockId -> editText(x, y, blockId) }
        val renderer = MathRenderer(this) { ink.invalidate() }
        mathRenderer = renderer
        ink.math = renderer
        ink.onMathTap = { x, y, blockId -> editMath(x, y, blockId) }
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
        bar.addView(textButton(R.string.select_tool) {
            ink.tool = if (ink.tool == InkView.Tool.SELECT) InkView.Tool.PEN else InkView.Tool.SELECT
            if (ink.tool != InkView.Tool.SELECT) ink.clearSelection()
            updateToolbar()
            if (ink.tool == InkView.Tool.SELECT) toast(getString(R.string.select_hint))
        })
        bar.addView(textButton(R.string.selection) { showSelectionActions() })
        bar.addView(textButton(R.string.math_tool) {
            ink.clearSelection()
            ink.tool = if (ink.tool == InkView.Tool.MATH) InkView.Tool.PEN else InkView.Tool.MATH
            updateToolbar()
            if (ink.tool == InkView.Tool.MATH) toast(getString(R.string.math_tool_hint))
        })
        bar.addView(textButton(R.string.text_tool) {
            ink.tool = if (ink.tool == InkView.Tool.TEXT) InkView.Tool.PEN else InkView.Tool.TEXT
            updateToolbar()
            if (ink.tool == InkView.Tool.TEXT) toast(getString(R.string.text_tool_hint))
        })
        bar.addView(textButton(R.string.paper) { cycleTemplate() })
        bar.addView(textButton(R.string.notebooks) { showNotebooks() })
        bar.addView(textButton(R.string.pages) { showPages() })
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

        // Before several notebooks existed, the only notebook lived in `notebook/`.
        val legacy = File(filesDir, "notebook")
        if (legacy.isDirectory && notebooksRoot.list().isNullOrEmpty()) legacy.renameTo(File(notebooksRoot, "nb-0001"))
        val dirs = notebookDirs()
        val current = prefs.getString("notebook", null)?.let { File(notebooksRoot, it) }?.takeIf { it.isDirectory }
            ?: dirs.firstOrNull() ?: File(notebooksRoot, "nb-0001")
        openNotebook(current, prefs.getString("pageId", null))
        updateToolbar()
    }

    override fun onPause() {
        super.onPause()
        main.removeCallbacks(saveRunnable)
        saveNow()
        pageIds.getOrNull(pageIndex)?.let { prefs.edit().putString("pageId", it).apply() }
        io.execute { runCatching { notebook.save() } }
    }

    override fun onDestroy() {
        stopSync()
        super.onDestroy()
        io.shutdown()
    }

    // ---------- notebooks ----------

    private fun notebookDirs(): List<File> =
        notebooksRoot.listFiles { f -> f.isDirectory }?.sortedBy { it.name } ?: emptyList()

    private var notebookDir: File? = null

    private fun openNotebook(dir: File, pageId: String? = null) {
        if (::notebook.isInitialized) {
            main.removeCallbacks(saveRunnable)
            saveNow()
            val old = notebook
            io.execute { runCatching { old.save() } }
        }
        notebook = try {
            NotebookDoc.open(dir.path, getString(R.string.untitled))
        } catch (e: CoreException) {
            toast(getString(R.string.open_failed, e.message ?: ""))
            NotebookDoc.open(File(notebooksRoot, "recovered-${System.currentTimeMillis()}").path, getString(R.string.untitled))
        }
        prefs.edit().putString("notebook", dir.name).apply()
        notebookDir = dir
        restartNotebookSync()
        pageIds = notebook.pageIds()
        pageIndex = -1
        goToPage(pageIds.indexOf(pageId).takeIf { it >= 0 } ?: 0)
    }

    /** All notebooks: tap to open, or create a new one. */
    private fun showNotebooks() {
        val dirs = notebookDirs()
        val titles = dirs.map { d ->
            val t = if (d.name == prefs.getString("notebook", null)) notebook.title()
            else runCatching { NotebookDoc.open(d.path, "").title() }.getOrDefault(d.name)
            t.ifEmpty { getString(R.string.untitled) }
        }
        val current = dirs.indexOfFirst { it.name == prefs.getString("notebook", null) }
        val labels = titles.mapIndexed { i, t -> if (i == current) "• $t" else t }.toTypedArray()
        AlertDialog.Builder(this)
            .setTitle(R.string.notebooks)
            .setItems(labels) { _, which -> if (which != current) openNotebook(dirs[which]) }
            .setPositiveButton(R.string.new_notebook) { _, _ ->
                askText(R.string.new_notebook, "") { title ->
                    val next = (dirs.mapNotNull { it.name.removePrefix("nb-").toIntOrNull() }.maxOrNull() ?: 0) + 1
                    val dir = File(notebooksRoot, "nb-%04d".format(next))
                    openNotebook(dir)
                    notebook.setTitle(title.ifBlank { getString(R.string.untitled) })
                }
            }
            .setNeutralButton(R.string.more) { _, _ -> showNotebookActions() }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    private fun showNotebookActions() {
        val actions = arrayOf(
            getString(R.string.rename), getString(R.string.export_kahie), getString(R.string.import_kahie),
            getString(R.string.export_pdf),
            getString(R.string.sync_share), getString(R.string.sync_join), getString(R.string.sync_settings),
        )
        AlertDialog.Builder(this)
            .setItems(actions) { _, which ->
                when (which) {
                    0 -> askText(R.string.rename, notebook.title()) { notebook.setTitle(it) }
                    1 -> startExport()
                    2 -> startImport()
                    3 -> startPdfExport()
                    4 -> shareNotebook()
                    5 -> joinNotebook()
                    6 -> syncSettings()
                }
            }
            .show()
    }

    // ---------- sync with a self-hosted relay (decision D9) ----------

    private var notebookSync: SyncSession? = null
    private var pageSync: SyncSession? = null

    private fun relayUrl() = prefs.getString("relayUrl", "")!!.trim()
    private fun relayToken() = prefs.getString("relayToken", "")!!
    /** Share code of the current notebook, or null when it is not synced. */
    private fun syncCode(): String? = notebookDir?.let { prefs.getString("sync_${it.name}", null) }

    private fun syncSettings() {
        val pad = (16 * resources.displayMetrics.density).toInt()
        val box = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(pad, pad / 2, pad, 0) }
        val url = android.widget.EditText(this).apply { hint = "wss://relay.example.org"; setText(relayUrl()); setSingleLine() }
        val token = android.widget.EditText(this).apply {
            hint = getString(R.string.sync_token)
            setText(relayToken())
            setSingleLine()
            inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_VARIATION_PASSWORD
        }
        box.addView(url)
        box.addView(token)
        AlertDialog.Builder(this)
            .setTitle(R.string.sync_settings)
            .setView(box)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                prefs.edit().putString("relayUrl", url.text.toString().trim()).putString("relayToken", token.text.toString()).apply()
                restartNotebookSync()
                restartPageSync()
            }
            .show()
    }

    /** Turn sync on for this notebook and show the code other devices use to join it. */
    private fun shareNotebook() {
        val dir = notebookDir ?: return
        if (relayUrl().isEmpty()) { syncSettings(); return }
        val code = syncCode() ?: ("n" + java.util.UUID.randomUUID().toString().replace("-", "").take(15)).also {
            prefs.edit().putString("sync_${dir.name}", it).apply()
            restartNotebookSync()
            restartPageSync()
        }
        AlertDialog.Builder(this)
            .setTitle(R.string.sync_share)
            .setMessage(getString(R.string.sync_code_is, code))
            .setPositiveButton(android.R.string.ok, null)
            .show()
    }

    private fun joinNotebook() {
        if (relayUrl().isEmpty()) { syncSettings(); return }
        askText(R.string.sync_join, "") { code ->
            if (!code.matches(Regex("[A-Za-z0-9_-]{1,40}"))) { toast(getString(R.string.sync_bad_code)); return@askText }
            val next = (notebookDirs().mapNotNull { it.name.removePrefix("nb-").toIntOrNull() }.maxOrNull() ?: 0) + 1
            val dir = File(notebooksRoot, "nb-%04d".format(next))
            NotebookDoc.join(dir.path).save()
            prefs.edit().putString("sync_${dir.name}", code).apply()
            openNotebook(dir)
        }
    }

    private val syncListenerFor: (Boolean) -> SyncListener = { isNotebook ->
        object : SyncListener {
            override fun onRemoteChange() {
                main.post {
                    if (isNotebook) {
                        val current = pageIds.getOrNull(pageIndex)
                        pageIds = notebook.pageIds()
                        when {
                            pageIndex < 0 || current == null -> goToPage(0)
                            current in pageIds -> pageIndex = pageIds.indexOf(current)
                            else -> goToPage(pageIndex.coerceAtMost(pageIds.size - 1))
                        }
                        if (pageIndex >= 0) pageLabel.text = getString(R.string.page_count, pageIndex + 1, pageIds.size)
                    } else {
                        ink.refresh()
                        updateHistoryButtons()
                    }
                }
            }
            override fun onStatus(status: SyncStatus) {
                if (status is SyncStatus.Offline && !isNotebook) main.post { syncState = status.detail }
                if (status is SyncStatus.Online && !isNotebook) main.post { syncState = null }
            }
        }
    }

    /** Last sync error, shown in the sync dialog; null when online. */
    private var syncState: String? = null

    private fun restartNotebookSync() {
        notebookSync?.stop(); notebookSync = null
        val code = syncCode() ?: return
        if (relayUrl().isEmpty()) return
        notebookSync = SyncSession.startNotebook(notebook, relayUrl(), relayToken(), code, syncListenerFor(true))
    }

    private fun restartPageSync() {
        pageSync?.stop(); pageSync = null
        val code = syncCode() ?: return
        val d = doc ?: return
        val pageId = pageIds.getOrNull(pageIndex) ?: return
        if (relayUrl().isEmpty()) return
        pageSync = SyncSession.start(d, relayUrl(), relayToken(), "$code-$pageId", syncListenerFor(false))
    }

    private fun stopSync() {
        notebookSync?.stop(); notebookSync = null
        pageSync?.stop(); pageSync = null
    }

    // ---------- .kahie export / import (system file picker) ----------

    private fun startExport() {
        saveNow()
        val name = notebook.title().ifBlank { getString(R.string.untitled) }.replace(Regex("[\\\\/:*?\"<>|]"), "_")
        val intent = android.content.Intent(android.content.Intent.ACTION_CREATE_DOCUMENT)
            .addCategory(android.content.Intent.CATEGORY_OPENABLE)
            .setType("application/octet-stream")
            .putExtra(android.content.Intent.EXTRA_TITLE, "$name.kahie")
        startActivityForResult(intent, REQ_EXPORT)
    }

    private fun startPdfExport() {
        saveNow()
        val name = notebook.title().ifBlank { getString(R.string.untitled) }.replace(Regex("[\\\\/:*?\"<>|]"), "_")
        val intent = android.content.Intent(android.content.Intent.ACTION_CREATE_DOCUMENT)
            .addCategory(android.content.Intent.CATEGORY_OPENABLE)
            .setType("application/pdf")
            .putExtra(android.content.Intent.EXTRA_TITLE, "$name.pdf")
        startActivityForResult(intent, REQ_EXPORT_PDF)
    }

    /** Vector PDF of every page (A4), after all formulas are rendered. */
    private fun exportPdf(uri: android.net.Uri) {
        val renderer = mathRenderer
        val currentId = pageIds.getOrNull(pageIndex)
        val pages = pageIds.map { id -> if (id == currentId && doc != null) doc!! else PageDoc.open(notebook.pagePath(id)) }
        val requests = pages.flatMap { PageRenderer.mathRequests(it) }.distinct()
        var remaining = requests.size
        fun write() {
            val pdf = android.graphics.pdf.PdfDocument()
            val w = 595
            val h = 842 // A4 in PostScript points
            pages.forEachIndexed { i, p ->
                val page = pdf.startPage(android.graphics.pdf.PdfDocument.PageInfo.Builder(w, h, i + 1).create())
                page.canvas.scale(w / PageRenderer.PAGE_W, h / PageRenderer.PAGE_H)
                PageRenderer.drawPage(page.canvas, p, renderer)
                pdf.finishPage(page)
            }
            io.execute {
                val result = runCatching { contentResolver.openOutputStream(uri)!!.use { pdf.writeTo(it) } }
                pdf.close()
                main.post {
                    result.onSuccess { toast(getString(R.string.exported)) }
                        .onFailure { toast(getString(R.string.transfer_failed, it.message ?: "")) }
                }
            }
        }
        if (renderer == null || remaining == 0) { write(); return }
        toast(getString(R.string.preparing_export))
        for ((latex, size, color) in requests) {
            renderer.request(latex, size, color) { if (--remaining == 0) write() }
        }
    }

    private fun startImport() {
        val intent = android.content.Intent(android.content.Intent.ACTION_OPEN_DOCUMENT)
            .addCategory(android.content.Intent.CATEGORY_OPENABLE)
            .setType("*/*")
        startActivityForResult(intent, REQ_IMPORT)
    }

    @Deprecated("Platform Activity API; this app does not use AndroidX Activity")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: android.content.Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        val uri = data?.data ?: return
        if (resultCode != RESULT_OK) return
        if (requestCode == REQ_EXPORT_PDF) { exportPdf(uri); return }
        val dir = notebookDir ?: return
        io.execute {
            val tmp = File(cacheDir, "transfer.kahie")
            val result = runCatching {
                when (requestCode) {
                    REQ_EXPORT -> {
                        exportKahie(dir.path, tmp.path)
                        contentResolver.openOutputStream(uri)!!.use { out -> tmp.inputStream().use { it.copyTo(out) } }
                        null
                    }
                    REQ_IMPORT -> {
                        contentResolver.openInputStream(uri)!!.use { inp -> tmp.outputStream().use { inp.copyTo(it) } }
                        File(importKahie(tmp.path, notebooksRoot.path))
                    }
                    else -> null
                }
            }
            tmp.delete()
            main.post {
                result.onSuccess { imported ->
                    if (imported != null) openNotebook(imported)
                    toast(getString(if (requestCode == REQ_EXPORT) R.string.exported else R.string.imported))
                }.onFailure { toast(getString(R.string.transfer_failed, it.message ?: "")) }
            }
        }
    }

    /** Create a maths block at (x, y) or edit the one tapped (LaTeX, rendered by MathJax). */
    private fun editMath(x: Float, y: Float, blockId: String?) {
        val d = doc ?: return
        val renderer = mathRenderer ?: return
        val existing = blockId?.let { id -> d.mathBlocks().firstOrNull { it.id == id } }
        val input = android.widget.EditText(this).apply {
            setText(existing?.latex ?: "")
            hint = "\\frac{a}{b} + \\sqrt{x}"
            setSelection(text.length)
        }
        AlertDialog.Builder(this)
            .setTitle(if (existing == null) R.string.add_math else R.string.edit_math)
            .setView(input)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val latex = input.text.toString().trim()
                val size = existing?.size ?: 24f
                val color = existing?.color ?: (ink.penColor and 0xFFFFFF).toUInt()
                if (latex.isEmpty()) {
                    if (existing != null) { d.setMath(existing.id, "", 0.0, 0.0); ink.refresh(); ink.onChanged() }
                    return@setPositiveButton
                }
                // Measure with the renderer first, so the block's bounds match what is drawn.
                renderer.request(latex, size, (color.toInt() and 0xFFFFFF) or (0xFF shl 24)) { r ->
                    if (r.error) toast(getString(R.string.math_error))
                    val w = if (r.width > 0f) r.width else 60f
                    val h = if (r.height > 0f) r.height else size * 1.4f
                    if (existing != null) d.setMath(existing.id, latex, w.toDouble(), h.toDouble())
                    else d.addMath(x.toDouble(), y.toDouble(), w.toDouble(), h.toDouble(), latex, size.toDouble(), color)
                    ink.refresh()
                    ink.onChanged()
                }
            }
            .show()
    }

    /** Duplicate / delete / deselect the current selection. */
    private fun showSelectionActions() {
        val sel = ink.selection
        val d = doc ?: return
        if (sel.isEmpty()) {
            toast(getString(R.string.nothing_selected))
            return
        }
        AlertDialog.Builder(this)
            .setTitle(resources.getQuantityString(R.plurals.selected_items, sel.size, sel.size))
            .setItems(arrayOf(getString(R.string.duplicate), getString(R.string.delete), getString(R.string.deselect))) { _, which ->
                when (which) {
                    0 -> ink.select(d.duplicateItems(sel, 24.0, 24.0))
                    1 -> { d.deleteItems(sel); ink.select(emptyList()) }
                    else -> ink.clearSelection()
                }
                ink.refresh()
                ink.onChanged()
            }
            .show()
    }

    /** Create a text block at (x, y) or edit the one tapped. */
    private fun editText(x: Float, y: Float, blockId: String?) {
        val d = doc ?: return
        val existing = blockId?.let { id -> d.textBlocks().firstOrNull { it.id == id } }
        val input = android.widget.EditText(this).apply {
            setText(existing?.text ?: "")
            minLines = 3
            gravity = Gravity.TOP
            setSelection(text.length)
        }
        val builder = AlertDialog.Builder(this)
            .setTitle(if (existing == null) R.string.add_text else R.string.edit_text)
            .setView(input)
            .setNegativeButton(android.R.string.cancel, null)
        if (casAvailable()) {
            // Evaluate the last line with the CAS and append "= result" to the text.
            builder.setNeutralButton(R.string.compute) { _, _ -> }
        }
        val dialog = builder
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val text = input.text.toString()
                if (existing != null) {
                    d.setText(existing.id, text, ink.measureText(text, existing.width, existing.size).toDouble())
                } else if (text.isNotBlank()) {
                    val width = 420f
                    val size = 20f
                    d.addText(x.toDouble(), y.toDouble(), width.toDouble(), ink.measureText(text, width, size).toDouble(), text, size.toDouble(), (penColor() and 0xFFFFFF).toUInt())
                }
                ink.refresh()
                ink.onChanged()
            }
            .create()
        dialog.setOnShowListener {
            dialog.getButton(AlertDialog.BUTTON_NEUTRAL)?.setOnClickListener {
                val line = input.text.toString().trimEnd().lines().lastOrNull()?.trim().orEmpty()
                if (line.isEmpty()) return@setOnClickListener
                io.execute {
                    val result = runCatching { casEval(line).text }
                    main.post {
                        result.onSuccess { input.append("\n= $it"); input.setSelection(input.text.length) }
                            .onFailure { toast(getString(R.string.compute_failed, it.message ?: "")) }
                    }
                }
            }
        }
        dialog.show()
    }

    private fun penColor(): Int = ink.penColor

    private fun askText(title: Int, initial: String, onOk: (String) -> Unit) {
        val input = android.widget.EditText(this).apply {
            setText(initial)
            setSingleLine()
            setSelection(text.length)
        }
        AlertDialog.Builder(this)
            .setTitle(title)
            .setView(input)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ -> onOk(input.text.toString().trim()) }
            .show()
    }

    // ---------- pages ----------

    private fun goToPage(i: Int) {
        if (i < 0 || i >= pageIds.size) return
        main.removeCallbacks(saveRunnable)
        saveNow()
        pageIndex = i
        doc = try {
            PageDoc.open(notebook.pagePath(pageIds[i]))
        } catch (e: CoreException) {
            toast(getString(R.string.open_failed, e.message ?: ""))
            PageDoc.newInMemory()
        }
        ink.doc = doc
        restartPageSync()
        pageLabel.text = getString(R.string.page_count, i + 1, pageIds.size)
        updateHistoryButtons()
    }

    /** Insert a page after the current one, with the same paper. */
    private fun addPage() {
        val template = doc?.template() ?: "lined"
        val id = notebook.addPage(pageIds.getOrNull(pageIndex))
        pageIds = notebook.pageIds()
        goToPage(pageIds.indexOf(id))
        doc?.setTemplate(template)
        ink.invalidate()
    }

    /** Outline of the notebook: tap a page to open it; delete the current page (restorable). */
    private fun showPages() {
        val entries = notebook.outline()
        val labels = entries.map { e ->
            val indent = "    ".repeat(e.depth.toInt())
            when (e.kind) {
                OutlineKind.SECTION -> indent + e.title
                OutlineKind.PAGE -> {
                    val n = pageIds.indexOf(e.pageId) + 1
                    val name = e.title.ifEmpty { getString(R.string.page_n, n) }
                    indent + (if (e.pageId == pageIds.getOrNull(pageIndex)) "• " else "") + name
                }
            }
        }.toTypedArray()
        AlertDialog.Builder(this)
            .setTitle(R.string.pages)
            .setItems(labels) { _, which ->
                entries[which].pageId?.let { id -> goToPage(pageIds.indexOf(id)) }
            }
            .setPositiveButton(R.string.add_page) { _, _ -> addPage() }
            .setNeutralButton(R.string.more) { _, _ ->
                AlertDialog.Builder(this)
                    .setItems(arrayOf(getString(R.string.delete_page), getString(R.string.trash))) { _, which ->
                        if (which == 0) confirmDeletePage() else showTrash()
                    }
                    .show()
            }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    /** Deleted pages and sections; tap one to restore it. */
    private fun showTrash() {
        val entries = notebook.trash()
        if (entries.isEmpty()) {
            toast(getString(R.string.trash_empty))
            return
        }
        val labels = entries.map { e -> e.title.ifEmpty { getString(R.string.untitled_page) } }.toTypedArray()
        AlertDialog.Builder(this)
            .setTitle(R.string.trash)
            .setItems(labels) { _, which ->
                val e = entries[which]
                notebook.restoreNode(e.node)
                pageIds = notebook.pageIds()
                e.pageId?.let { id -> goToPage(pageIds.indexOf(id)) }
                toast(getString(R.string.restored))
            }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    private fun confirmDeletePage() {
        if (pageIds.size <= 1) {
            toast(getString(R.string.last_page))
            return
        }
        val id = pageIds.getOrNull(pageIndex) ?: return
        AlertDialog.Builder(this)
            .setMessage(R.string.delete_page_confirm)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(R.string.delete_page) { _, _ ->
                val node = notebook.outline().firstOrNull { it.pageId == id }?.node ?: return@setPositiveButton
                notebook.deleteNode(node)
                pageIds = notebook.pageIds()
                goToPage(pageIndex.coerceAtMost(pageIds.size - 1))
            }
            .show()
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
        ink.clearSelection()
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
