package app.kahie

import android.app.Activity
import android.app.Dialog
import android.graphics.Typeface
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

/**
 * A full-screen panel over the notebook (library, page list, search): a top bar with a title
 * and actions, and a scrolling body. Built from plain framework views like the rest of the app.
 */
open class Screen(protected val activity: Activity) {
    protected val dp = activity.resources.displayMetrics.density
    protected fun px(v: Int) = (v * dp).toInt()

    protected val dialog = Dialog(activity, android.R.style.Theme_Material_Light_NoActionBar)
    protected val root = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
        setBackgroundColor(BG)
    }
    private val bar = LinearLayout(activity).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        setPadding(px(8), px(6), px(8), px(6))
    }
    protected val titleView = TextView(activity).apply {
        textSize = 20f
        setTextColor(INK)
        typeface = Typeface.DEFAULT_BOLD
        setPadding(px(8), 0, px(8), 0)
        maxLines = 1
        ellipsize = android.text.TextUtils.TruncateAt.START
    }
    private val actions = LinearLayout(activity).apply { orientation = LinearLayout.HORIZONTAL }
    /** Optional row under the top bar (search field, breadcrumb…). */
    protected val header = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(px(12), 0, px(12), 0)
    }
    protected val scroll = ScrollView(activity)
    protected val body = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(px(12), px(4), px(12), px(24))
    }

    init {
        bar.addView(titleView, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        val actionScroll = HorizontalScrollView(activity).apply { isHorizontalScrollBarEnabled = false; addView(actions) }
        bar.addView(actionScroll, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        root.addView(bar)
        root.addView(header)
        scroll.addView(body)
        root.addView(scroll, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        dialog.setContentView(root)
        dialog.window?.setLayout(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
    }

    /** A text action in the top bar. */
    protected fun action(label: String, onClick: () -> Unit): TextView = TextView(activity).apply {
        text = label
        textSize = 15f
        setTextColor(ACCENT)
        setPadding(px(10), px(8), px(10), px(8))
        isClickable = true
        contentDescription = label
        setOnClickListener { onClick() }
        actions.addView(this)
    }

    protected fun clearActions() = actions.removeAllViews()

    protected fun label(text: CharSequence, size: Float = 15f, color: Int = INK, bold: Boolean = false) = TextView(activity).apply {
        this.text = text
        textSize = size
        setTextColor(color)
        if (bold) typeface = Typeface.DEFAULT_BOLD
    }

    protected fun divider() = View(activity).apply {
        setBackgroundColor(0x14000000)
        layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, maxOf(1, px(1) / 2))
    }

    open fun show() = dialog.show()

    fun dismiss() = dialog.dismiss()

    companion object {
        const val BG = 0xFFF6F4EF.toInt()
        const val INK = 0xFF1D1D1F.toInt()
        const val MUTED = 0xFF6B6F78.toInt()
        const val ACCENT = 0xFF2B59C3.toInt()
        const val HIGHLIGHT = 0x66FFE066
    }
}
