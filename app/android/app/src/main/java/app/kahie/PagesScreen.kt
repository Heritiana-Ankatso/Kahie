package app.kahie

import android.app.Activity
import android.content.ClipData
import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.Paint
import android.graphics.PixelFormat
import android.graphics.drawable.Drawable
import android.view.DragEvent
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.Toast
import uniffi.nb_ffi.NotebookDoc
import uniffi.nb_ffi.OutlineEntry
import uniffi.nb_ffi.OutlineKind
import uniffi.nb_ffi.PageDoc

/**
 * The page list (P1-03): sections and pages with thumbnails. Tap a page to open it, a section
 * for its actions; long-press a page or a section and drag it to reorder (drop on the top half
 * of a row to put it before, the bottom half to put it after, the middle of a section to put
 * it inside).
 */
class PagesScreen(
    activity: Activity,
    private val notebook: NotebookDoc,
    private val currentPageId: String?,
    private val openDoc: PageDoc?,
    private val host: Host,
) : Screen(activity) {

    interface Host {
        fun openPage(pageId: String)
        fun addPage()
        fun more()
        fun search()
        fun section(entries: List<OutlineEntry>, index: Int)
        /** The order of pages changed (the activity refreshes its page list). */
        fun reordered()
    }

    private enum class Zone { BEFORE, INTO, AFTER }

    init {
        titleView.text = activity.getString(R.string.pages)
        action(activity.getString(R.string.add_page)) { dismiss(); host.addPage() }
        action(activity.getString(R.string.search)) { dismiss(); host.search() }
        action(activity.getString(R.string.more)) { dismiss(); host.more() }
        header.addView(label(activity.getString(R.string.pages_drag_hint), 13f, MUTED))
    }

    override fun show() {
        render()
        super.show()
        // Bring the open page into view.
        scroll.post {
            val row = (0 until body.childCount).map { body.getChildAt(it) }.firstOrNull { it.tag == currentPageId }
            row?.let { scroll.scrollTo(0, (it.top - px(80)).coerceAtLeast(0)) }
        }
    }

    private fun render() {
        body.removeAllViews()
        val entries = notebook.outline()
        var number = 0
        entries.forEachIndexed { i, e ->
            val row = if (e.kind == OutlineKind.PAGE) pageRow(e, ++number) else sectionRow(entries, i)
            row.tag = e.pageId ?: e.node
            row.setOnLongClickListener { v ->
                v.startDragAndDrop(ClipData.newPlainText("kahie-node", e.node), View.DragShadowBuilder(v), e.node, 0)
                true
            }
            row.setOnDragListener { v, ev -> onDrag(v, ev, e) }
            body.addView(row)
        }
    }

    private fun indent(e: OutlineEntry) = px(16) * e.depth.toInt()

    private fun pageRow(e: OutlineEntry, number: Int) = LinearLayout(activity).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        setPadding(indent(e) + px(4), px(6), px(4), px(6))
        if (e.pageId == currentPageId) setBackgroundColor(0x1A2B59C3)
        val thumbW = px(64)
        val thumb = ImageView(activity).apply {
            scaleType = ImageView.ScaleType.FIT_CENTER
            setBackgroundColor(0xFFFFFFFF.toInt())
            contentDescription = activity.getString(R.string.page_n, number)
        }
        addView(thumb, LinearLayout.LayoutParams(thumbW, (thumbW * 1.414f).toInt()))
        val id = e.pageId
        if (id != null) Thumbnails.load(activity, thumb, notebook.pagePath(id), thumbW, if (id == currentPageId) openDoc else null)
        val text = LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(px(12), 0, 0, 0)
            addView(label(activity.getString(R.string.page_n, number), 12f, MUTED))
            addView(label(e.title.ifEmpty { activity.getString(R.string.untitled_page) }, 16f, if (e.title.isEmpty()) MUTED else INK, e.pageId == currentPageId))
        }
        addView(text, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        isClickable = true
        setOnClickListener { id?.let { dismiss(); host.openPage(it) } }
    }

    private fun sectionRow(entries: List<OutlineEntry>, i: Int): View {
        val e = entries[i]
        return label("▸  " + e.title.ifEmpty { activity.getString(R.string.new_section) }, 17f, INK, bold = true).apply {
            setPadding(indent(e) + px(4), px(14), px(4), px(8))
            isClickable = true
            setOnClickListener { dismiss(); host.section(entries, i) }
        }
    }

    private fun zone(v: View, y: Float, e: OutlineEntry): Zone {
        val f = y / v.height.coerceAtLeast(1)
        return if (e.kind == OutlineKind.SECTION) {
            when { f < 0.3f -> Zone.BEFORE; f > 0.8f -> Zone.AFTER; else -> Zone.INTO }
        } else if (f < 0.5f) Zone.BEFORE else Zone.AFTER
    }

    private fun onDrag(v: View, ev: DragEvent, target: OutlineEntry): Boolean {
        val dragged = ev.localState as? String ?: return false
        when (ev.action) {
            DragEvent.ACTION_DRAG_STARTED -> return true
            DragEvent.ACTION_DRAG_LOCATION -> {
                v.foreground = if (dragged == target.node) null else Mark(zone(v, ev.y, target))
                // Scroll when the finger nears the top or bottom of the list.
                val inView = v.top + ev.y - scroll.scrollY
                if (inView < px(64)) scroll.smoothScrollBy(0, -px(24))
                if (inView > scroll.height - px(64)) scroll.smoothScrollBy(0, px(24))
            }
            DragEvent.ACTION_DRAG_EXITED, DragEvent.ACTION_DRAG_ENDED -> v.foreground = null
            DragEvent.ACTION_DROP -> {
                v.foreground = null
                if (dragged == target.node) return false
                val ok = when (zone(v, ev.y, target)) {
                    Zone.INTO -> notebook.moveNode(dragged, target.node, 0u)
                    Zone.BEFORE -> notebook.moveNextTo(dragged, target.node, false)
                    Zone.AFTER -> notebook.moveNextTo(dragged, target.node, true)
                }
                if (!ok) Toast.makeText(activity, R.string.pages_cannot_move, Toast.LENGTH_SHORT).show()
                host.reordered()
                v.post { render() }
                return ok
            }
        }
        return true
    }

    /** Where a drop would land: a line above or below the row, or the whole row (into a section). */
    private inner class Mark(private val zone: Zone) : Drawable() {
        private val paint = Paint().apply { color = ACCENT; strokeWidth = 3 * dp }
        private val fill = Paint().apply { color = 0x222B59C3 }
        override fun draw(c: Canvas) {
            val b = bounds
            when (zone) {
                Zone.BEFORE -> c.drawLine(b.left.toFloat(), b.top + 1.5f * dp, b.right.toFloat(), b.top + 1.5f * dp, paint)
                Zone.AFTER -> c.drawLine(b.left.toFloat(), b.bottom - 1.5f * dp, b.right.toFloat(), b.bottom - 1.5f * dp, paint)
                Zone.INTO -> c.drawRect(b, fill)
            }
        }
        override fun setAlpha(alpha: Int) {}
        override fun setColorFilter(colorFilter: ColorFilter?) {}
        @Deprecated("Deprecated in Java")
        override fun getOpacity() = PixelFormat.TRANSLUCENT
    }
}
