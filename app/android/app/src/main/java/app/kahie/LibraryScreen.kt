package app.kahie

import android.app.Activity
import android.app.AlertDialog
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import uniffi.nb_ffi.LibraryDoc
import uniffi.nb_ffi.LibraryEntry
import uniffi.nb_ffi.LibraryKind
import java.io.File

/**
 * The library (P1-02): folders and notebooks with covers (first page), most recently opened
 * first, as a grid or a list. Tap a notebook to open it, a folder to go in; long-press for
 * rename / move / delete. The library trash keeps deleted notebooks and folders for 30 days.
 */
class LibraryScreen(
    activity: Activity,
    private val notebooksRoot: File,
    private val library: LibraryDoc,
    private val host: Host,
) : Screen(activity) {

    /** What the library asks of the activity that owns the open notebook. */
    interface Host {
        /** The directory name of the open notebook. */
        fun currentDir(): String?
        fun open(dir: File)
        /** Create a notebook (asks for its title) and file it in `folder`. */
        fun newNotebook(folder: String?)
        fun renameNotebook(dir: String, title: String)
        /** The notebook in `dir` went to the trash (the activity opens another if it was open). */
        fun deleted(dirs: List<String>)
        fun more()
        fun search()
    }

    private val prefs = activity.getSharedPreferences("kahie", Activity.MODE_PRIVATE)
    private var folder: String? = null
    private var showingTrash = false
    private var asList = prefs.getBoolean("libraryList", false)

    init {
        dialog.setOnKeyListener { _, code, ev ->
            if (code == KeyEvent.KEYCODE_BACK && ev.action == KeyEvent.ACTION_UP && (showingTrash || folder != null)) {
                if (showingTrash) showingTrash = false else folder = library.path(folder!!).lastOrNull()?.node
                render()
                true
            } else {
                false
            }
        }
    }

    override fun show() {
        folder = host.currentDir()?.let { library.folderOf(it) }
        render()
        super.show()
    }

    private fun str(id: Int, vararg args: Any) = activity.getString(id, *args)

    private fun render() {
        clearActions()
        header.removeAllViews()
        body.removeAllViews()
        if (showingTrash) return renderTrash()
        val path = folder?.let { f -> library.path(f) + library.folders().filter { it.node == f } } ?: emptyList()
        titleView.text = if (path.isEmpty()) str(R.string.notebooks) else path.last().title
        if (folder != null) action(str(R.string.lib_up)) { folder = path.dropLast(1).lastOrNull()?.node; render() }
        action(str(R.string.search)) { host.search() }
        action(str(R.string.new_notebook)) { dismiss(); host.newNotebook(folder) }
        action(str(R.string.lib_new_folder)) {
            ask(R.string.lib_new_folder, "") { t -> if (t.isNotEmpty()) { library.addFolder(folder, t); render() } }
        }
        action(str(if (asList) R.string.lib_grid else R.string.lib_list)) {
            asList = !asList
            prefs.edit().putBoolean("libraryList", asList).apply()
            render()
        }
        action(str(R.string.trash)) { showingTrash = true; render() }
        action(str(R.string.more)) { dismiss(); host.more() }
        if (path.isNotEmpty()) {
            header.addView(label((listOf(str(R.string.notebooks)) + path.map { it.title }).joinToString("  ›  "), 13f, MUTED))
        }
        val entries = library.list(folder)
        if (entries.isEmpty()) {
            body.addView(label(str(R.string.lib_folder_empty), 15f, MUTED).apply { setPadding(0, px(24), 0, 0) })
            return
        }
        if (asList) entries.forEach { body.addView(row(it)) } else grid(entries)
    }

    private fun grid(entries: List<LibraryEntry>) {
        val widthDp = activity.resources.displayMetrics.widthPixels / dp
        val columns = ((widthDp - 24) / 170).toInt().coerceIn(2, 8)
        val tileW = ((activity.resources.displayMetrics.widthPixels - px(24)) / columns) - px(12)
        entries.chunked(columns).forEach { chunk ->
            val line = LinearLayout(activity).apply { orientation = LinearLayout.HORIZONTAL }
            chunk.forEach { e -> line.addView(tile(e, tileW), LinearLayout.LayoutParams(tileW + px(12), ViewGroup.LayoutParams.WRAP_CONTENT)) }
            body.addView(line)
        }
    }

    private fun cover(e: LibraryEntry, w: Int, h: Int): View {
        if (e.kind == LibraryKind.FOLDER) {
            return FrameLayout(activity).apply {
                background = GradientDrawable().apply { cornerRadius = px(8).toFloat(); setColor(0xFFE3E8F4.toInt()) }
                addView(label(activity.resources.getQuantityString(R.plurals.lib_items, e.count.toInt(), e.count.toInt()), 13f, MUTED).apply {
                    gravity = Gravity.CENTER
                }, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
                layoutParams = LinearLayout.LayoutParams(w, h)
            }
        }
        return ImageView(activity).apply {
            scaleType = ImageView.ScaleType.FIT_CENTER
            background = GradientDrawable().apply { setColor(0xFFFFFFFF.toInt()); setStroke(maxOf(1, px(1)), 0x22000000) }
            layoutParams = LinearLayout.LayoutParams(w, h)
            contentDescription = title(e)
            Thumbnails.loadCover(activity, this, File(notebooksRoot, e.dir ?: ""), w)
        }
    }

    private fun title(e: LibraryEntry) = e.title.ifEmpty {
        str(if (e.kind == LibraryKind.FOLDER) R.string.lib_folder else R.string.untitled)
    }

    private fun subtitle(e: LibraryEntry): String = when {
        e.kind == LibraryKind.FOLDER -> str(R.string.lib_folder)
        e.dir == host.currentDir() -> str(R.string.lib_open_now)
        e.openedSecs > 0 -> android.text.format.DateUtils.getRelativeTimeSpanString(e.openedSecs * 1000).toString()
        else -> ""
    }

    private fun tile(e: LibraryEntry, w: Int) = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(px(6), px(8), px(6), px(8))
        addView(cover(e, w, (w * 1.414f).toInt()))
        addView(label(title(e), 15f, INK, bold = e.dir == host.currentDir()).apply { maxLines = 2; setPadding(0, px(6), 0, 0) })
        addView(label(subtitle(e), 12f, MUTED))
        bind(this, e)
    }

    private fun row(e: LibraryEntry) = LinearLayout(activity).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        setPadding(0, px(6), 0, px(6))
        addView(cover(e, px(48), px(68)))
        val text = LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(px(12), 0, 0, 0)
            addView(label(title(e), 16f, INK, bold = e.dir == host.currentDir()))
            addView(label(subtitle(e), 12f, MUTED))
        }
        addView(text, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        bind(this, e)
    }

    private fun bind(v: View, e: LibraryEntry) {
        v.isClickable = true
        v.contentDescription = title(e)
        v.setOnClickListener {
            if (e.kind == LibraryKind.FOLDER) { folder = e.node; render() } else {
                dismiss()
                host.open(File(notebooksRoot, e.dir!!))
            }
        }
        v.setOnLongClickListener { actionsFor(e); true }
    }

    private fun actionsFor(e: LibraryEntry) {
        AlertDialog.Builder(activity)
            .setTitle(title(e))
            .setItems(arrayOf(str(R.string.rename), str(R.string.lib_move_to_folder), str(R.string.lib_delete))) { _, which ->
                when (which) {
                    0 -> ask(R.string.rename, e.title) { t ->
                        if (e.kind == LibraryKind.FOLDER) { if (t.isNotEmpty()) library.rename(e.node, t) } else host.renameNotebook(e.dir!!, t)
                        render()
                    }
                    1 -> chooseFolder(e)
                    else -> {
                        val dirs = if (e.kind == LibraryKind.NOTEBOOK) listOfNotNull(e.dir) else folderDirs(e.node)
                        library.deleteNode(e.node)
                        android.widget.Toast.makeText(activity, str(R.string.lib_moved_to_trash), android.widget.Toast.LENGTH_SHORT).show()
                        host.deleted(dirs)
                        render()
                    }
                }
            }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    /** Notebook directories inside folder `node` (any depth). */
    private fun folderDirs(node: String): List<String> = library.list(node).flatMap { c ->
        if (c.kind == LibraryKind.FOLDER) folderDirs(c.node) else listOfNotNull(c.dir)
    }

    private fun chooseFolder(e: LibraryEntry) {
        val folders = library.folders().filter { it.node != e.node }
        val labels = listOf(str(R.string.top_level)) + folders.map { "    ".repeat(it.depth.toInt()) + title(it) }
        AlertDialog.Builder(activity)
            .setTitle(R.string.lib_move_to_folder)
            .setItems(labels.toTypedArray()) { _, which ->
                val target = if (which == 0) null else folders[which - 1].node
                if (!library.moveTo(e.node, target)) {
                    android.widget.Toast.makeText(activity, str(R.string.lib_cannot_move), android.widget.Toast.LENGTH_SHORT).show()
                }
                render()
            }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    private fun renderTrash() {
        titleView.text = str(R.string.trash)
        action(str(R.string.lib_back)) { showingTrash = false; render() }
        val entries = library.trash()
        if (entries.isNotEmpty()) {
            action(str(R.string.empty_trash)) {
                AlertDialog.Builder(activity)
                    .setMessage(R.string.lib_empty_trash_confirm)
                    .setNegativeButton(android.R.string.cancel, null)
                    .setPositiveButton(R.string.empty_trash) { _, _ -> library.emptyTrash(); render() }
                    .show()
            }
        }
        header.addView(label(str(R.string.lib_trash_note), 13f, MUTED))
        if (entries.isEmpty()) {
            body.addView(label(str(R.string.trash_empty), 15f, MUTED).apply { setPadding(0, px(24), 0, 0) })
            return
        }
        for (e in entries) {
            val days = e.deletedSecs?.let { ((System.currentTimeMillis() / 1000 - it) / 86_400).toInt() } ?: 0
            val left = (30 - days).coerceAtLeast(0)
            val v = LinearLayout(activity).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(0, px(10), 0, px(10))
                addView(label(title(e), 16f))
                addView(label(activity.resources.getQuantityString(R.plurals.lib_days_left, left, left), 12f, MUTED))
                isClickable = true
                setOnClickListener {
                    AlertDialog.Builder(activity)
                        .setTitle(title(e))
                        .setItems(arrayOf(str(R.string.lib_restore))) { _, _ ->
                            library.restoreNode(e.node)
                            android.widget.Toast.makeText(activity, str(R.string.restored), android.widget.Toast.LENGTH_SHORT).show()
                            render()
                        }
                        .setNegativeButton(android.R.string.cancel, null)
                        .show()
                }
            }
            body.addView(v)
            body.addView(divider())
        }
    }

    private fun ask(title: Int, initial: String, onOk: (String) -> Unit) {
        val input = android.widget.EditText(activity).apply { setText(initial); setSingleLine(); setSelection(text.length) }
        AlertDialog.Builder(activity)
            .setTitle(title)
            .setView(input)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ -> onOk(input.text.toString().trim()) }
            .show()
    }
}
