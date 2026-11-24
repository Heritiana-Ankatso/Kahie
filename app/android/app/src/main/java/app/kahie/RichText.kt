package app.kahie

import android.app.Activity
import android.app.AlertDialog
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Typeface
import android.text.Editable
import android.text.Layout
import android.text.Spannable
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.StaticLayout
import android.text.TextPaint
import android.text.style.ForegroundColorSpan
import android.text.style.LeadingMarginSpan
import android.text.style.MetricAffectingSpan
import android.text.style.StyleSpan
import android.text.style.UnderlineSpan
import android.view.Gravity
import android.widget.EditText
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.TextView
import uniffi.nb_ffi.RichSpan

/**
 * Rich text blocks (P1-17): conversion between the core's spans (UTF-16 offsets) and Android
 * spans, layout for drawing and export, and the editor dialog with its formatting bar.
 */
object RichText {
    /** Heading paragraph: larger and bold. */
    class HeadingSpan(val level: Int) : MetricAffectingSpan() {
        private val factor = when (level) { 1 -> 1.6f; 2 -> 1.35f; else -> 1.15f }
        private fun apply(p: TextPaint) {
            p.textSize *= factor
            p.isFakeBoldText = true
        }
        override fun updateDrawState(tp: TextPaint) = apply(tp)
        override fun updateMeasureState(tp: TextPaint) = apply(tp)
    }

    /** List paragraph: a bullet or a number in the left margin. */
    class ListSpan(val kind: Int, val number: Int, private val indent: Int) : LeadingMarginSpan {
        override fun getLeadingMargin(first: Boolean) = indent
        override fun drawLeadingMargin(
            c: Canvas, p: Paint, x: Int, dir: Int, top: Int, baseline: Int, bottom: Int,
            text: CharSequence, start: Int, end: Int, first: Boolean, layout: Layout?,
        ) {
            val spanned = text as? Spanned ?: return
            if (!first || spanned.getSpanStart(this) != start) return
            val label = if (kind == 2) "$number." else "•"
            val w = p.measureText(label)
            c.drawText(label, x + dir * (indent - w - indent * 0.2f).coerceAtLeast(0f), baseline.toFloat(), p)
        }
    }

    private fun opaque(c: UInt) = (c.toInt() and 0xFFFFFF) or (0xFF shl 24)

    /** Paragraph [start, end) around `at` (end excludes the newline). */
    private fun paragraph(s: CharSequence, at: Int): IntRange {
        var a = at.coerceIn(0, s.length)
        while (a > 0 && s[a - 1] != '\n') a--
        var b = at.coerceIn(0, s.length)
        while (b < s.length && s[b] != '\n') b++
        return a until b
    }

    /** Put `spans` on `out` (whose text they describe); `size` sets the list indent. */
    fun apply(out: Spannable, spans: List<RichSpan>, size: Float) {
        val flags = Spanned.SPAN_EXCLUSIVE_INCLUSIVE
        val n = out.length
        var number = 0
        var lastListEnd = -1
        for (sp in spans.sortedBy { it.start }) {
            val a = sp.start.toInt().coerceIn(0, n)
            val b = sp.end.toInt().coerceIn(a, n)
            if (a == b) continue
            if (sp.bold || sp.italic) {
                val style = if (sp.bold && sp.italic) Typeface.BOLD_ITALIC else if (sp.bold) Typeface.BOLD else Typeface.ITALIC
                out.setSpan(StyleSpan(style), a, b, flags)
            }
            if (sp.underline) out.setSpan(UnderlineSpan(), a, b, flags)
            sp.color?.let { out.setSpan(ForegroundColorSpan(opaque(it)), a, b, flags) }
            if (sp.heading.toInt() > 0) out.setSpan(HeadingSpan(sp.heading.toInt()), a, b, flags)
            if (sp.list.toInt() > 0) {
                // One list span per paragraph, numbered within a run of numbered paragraphs.
                var p = a
                while (p < b) {
                    val para = paragraph(out, p)
                    number = if (lastListEnd >= 0 && para.first <= lastListEnd + 1) number + 1 else 1
                    out.setSpan(ListSpan(sp.list.toInt(), number, (size * 1.4f).toInt()), para.first, maxOf(para.last + 1, para.first), Spanned.SPAN_EXCLUSIVE_INCLUSIVE)
                    lastListEnd = para.last + 1
                    p = para.last + 2
                }
            }
        }
    }

    fun spannable(text: String, spans: List<RichSpan>, size: Float): SpannableStringBuilder =
        SpannableStringBuilder(text).also { apply(it, spans, size) }

    /** Formatting on `s` back as core spans (merged runs, plain runs left out). */
    fun spansOf(s: Spanned): List<RichSpan> {
        val n = s.length
        val cuts = sortedSetOf(0, n)
        for (o in s.getSpans(0, n, Any::class.java)) {
            if (o is StyleSpan || o is UnderlineSpan || o is ForegroundColorSpan || o is HeadingSpan || o is ListSpan) {
                cuts.add(s.getSpanStart(o).coerceIn(0, n)); cuts.add(s.getSpanEnd(o).coerceIn(0, n))
            }
        }
        val out = ArrayList<RichSpan>()
        val c = cuts.toList()
        for (i in 0 until c.size - 1) {
            val a = c[i]
            val b = c[i + 1]
            if (a >= b) continue
            var bold = false; var italic = false; var underline = false
            var color: UInt? = null; var heading = 0; var list = 0
            for (o in s.getSpans(a, b, Any::class.java)) {
                if (s.getSpanStart(o) > a || s.getSpanEnd(o) < b) continue
                when (o) {
                    is StyleSpan -> { if (o.style and Typeface.BOLD != 0) bold = true; if (o.style and Typeface.ITALIC != 0) italic = true }
                    is UnderlineSpan -> underline = true
                    is ForegroundColorSpan -> color = (o.foregroundColor and 0xFFFFFF).toUInt()
                    is HeadingSpan -> heading = o.level
                    is ListSpan -> list = o.kind
                }
            }
            val span = RichSpan(a.toUInt(), b.toUInt(), bold, italic, underline, color, heading.toUByte(), list.toUByte())
            if (!bold && !italic && !underline && color == null && heading == 0 && list == 0) continue
            val last = out.lastOrNull()
            if (last != null && last.end == span.start && last.copy(start = span.start, end = span.end) == span) {
                out[out.size - 1] = last.copy(end = span.end)
            } else {
                out.add(span)
            }
        }
        return out
    }

    /** Laid-out block, as drawn on the page and exported. */
    fun layout(text: String, spans: List<RichSpan>, width: Float, size: Float, color: UInt): StaticLayout {
        val paint = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
            textSize = size
            this.color = opaque(color)
        }
        val s: CharSequence = if (spans.isEmpty()) text else spannable(text, spans, size)
        return StaticLayout.Builder.obtain(s, 0, s.length, paint, width.toInt().coerceAtLeast(20)).build()
    }

    /**
     * Natural width of a block (its longest line, unwrapped): the width of an auto-width
     * text box (P1-17). A box is auto-width while its width equals this (within a pixel).
     */
    fun naturalWidth(text: String, spans: List<RichSpan>, size: Float): Float {
        val l = layout(text, spans, 100_000f, size, 0u)
        var w = 0f
        for (i in 0 until l.lineCount) w = maxOf(w, l.getLineWidth(i))
        return kotlin.math.ceil(w + 2f).coerceAtLeast(20f)
    }

    fun isAutoWidth(text: String, spans: List<RichSpan>, size: Float, width: Float): Boolean =
        kotlin.math.abs(naturalWidth(text, spans, size) - width) <= 1.5f

    // ---------- editing ----------

    /** Apply `f` to the formatting of [a, b) of `spans` (a text of length `n`). */
    fun restyle(spans: List<RichSpan>, n: Int, a: Int, b: Int, f: (RichSpan) -> RichSpan): List<RichSpan> {
        val cuts = sortedSetOf(0, n, a.coerceIn(0, n), b.coerceIn(0, n))
        for (s in spans) { cuts.add(s.start.toInt().coerceIn(0, n)); cuts.add(s.end.toInt().coerceIn(0, n)) }
        val c = cuts.toList()
        val out = ArrayList<RichSpan>()
        for (i in 0 until c.size - 1) {
            val x = c[i]
            val y = c[i + 1]
            if (x >= y) continue
            val base = spans.lastOrNull { it.start.toInt() <= x && it.end.toInt() >= y }
                ?: RichSpan(0u, 0u, false, false, false, null, 0u, 0u)
            var s = base.copy(start = x.toUInt(), end = y.toUInt())
            if (x >= a && y <= b) s = f(s).copy(start = x.toUInt(), end = y.toUInt())
            out.add(s)
        }
        return out.filter { it.bold || it.italic || it.underline || it.color != null || it.heading.toInt() != 0 || it.list.toInt() != 0 }
    }

    /** Whether every character of [a, b) satisfies `test`. */
    fun all(spans: List<RichSpan>, a: Int, b: Int, test: (RichSpan) -> Boolean): Boolean {
        if (a >= b) return false
        var at = a
        for (s in spans.sortedBy { it.start }) {
            val x = s.start.toInt()
            val y = s.end.toInt()
            if (y <= at || !test(s)) continue
            if (x > at) return false
            at = maxOf(at, y)
            if (at >= b) return true
        }
        return at >= b
    }

    private fun Spannable.clearRich() {
        for (o in getSpans(0, length, Any::class.java)) {
            if (o is StyleSpan || o is UnderlineSpan || o is ForegroundColorSpan || o is HeadingSpan || o is ListSpan) removeSpan(o)
        }
    }

    private fun setSpans(e: Editable, spans: List<RichSpan>, size: Float) {
        e.clearRich()
        apply(e, spans, size)
    }

    /**
     * The rich text editor: an EditText with a bar for bold, italic, underline, headings,
     * bullet and numbered lists and colour. Formatting applies to the selection (headings
     * and lists to whole paragraphs). `onOk` gets the text and its spans.
     */
    fun edit(
        activity: Activity,
        title: Int,
        text: String,
        spans: List<RichSpan>,
        size: Float,
        colors: IntArray,
        compute: ((String, (String) -> Unit) -> Unit)?,
        /** Extra controls shown under the field (e.g. CAS operations), given the field. */
        extra: ((EditText) -> android.view.View?)? = null,
        onOk: (String, List<RichSpan>) -> Unit,
    ) {
        val dp = activity.resources.displayMetrics.density
        val input = EditText(activity).apply {
            setText(spannable(text, spans, textSize / dp))
            minLines = 4
            gravity = Gravity.TOP
            setSelection(this.text.length)
        }
        fun current(): List<RichSpan> = spansOf(input.text)
        fun range(): Pair<Int, Int> {
            val a = minOf(input.selectionStart, input.selectionEnd).coerceAtLeast(0)
            val b = maxOf(input.selectionStart, input.selectionEnd).coerceAtLeast(0)
            return a to b
        }
        fun paragraphs(): Pair<Int, Int> {
            val (a, b) = range()
            return paragraph(input.text, a).first to paragraph(input.text, b).last + 1
        }
        fun restyleSelection(paragraph: Boolean, on: (RichSpan) -> Boolean, f: (RichSpan, Boolean) -> RichSpan) {
            val (a, b) = if (paragraph) paragraphs() else range()
            if (a >= b) return
            val spansNow = current()
            val turnOn = !all(spansNow, a, b, on)
            val selA = input.selectionStart
            val selB = input.selectionEnd
            setSpans(input.text, restyle(spansNow, input.text.length, a, b) { f(it, turnOn) }, input.textSize / dp)
            input.setSelection(selA, selB)
        }
        val bar = LinearLayout(activity).apply { orientation = LinearLayout.HORIZONTAL }
        fun button(label: String, desc: Int, style: Int = Typeface.NORMAL, onClick: () -> Unit) = TextView(activity).apply {
            this.text = label
            contentDescription = activity.getString(desc)
            tooltipText = activity.getString(desc)
            textSize = 16f
            setTypeface(typeface, style)
            gravity = Gravity.CENTER
            minWidth = (40 * dp).toInt()
            minHeight = (40 * dp).toInt()
            setOnClickListener { onClick() }
        }.also { bar.addView(it) }
        button("B", R.string.format_bold, Typeface.BOLD) { restyleSelection(false, { it.bold }) { s, on -> s.copy(bold = on) } }
        button("I", R.string.format_italic, Typeface.ITALIC) { restyleSelection(false, { it.italic }) { s, on -> s.copy(italic = on) } }
        button("U", R.string.format_underline) { restyleSelection(false, { it.underline }) { s, on -> s.copy(underline = on) } }
        for (level in 1..3) {
            button("H$level", R.string.format_heading) {
                restyleSelection(true, { it.heading.toInt() == level }) { s, on -> s.copy(heading = (if (on) level else 0).toUByte()) }
            }
        }
        button("•", R.string.format_bullets) { restyleSelection(true, { it.list.toInt() == 1 }) { s, on -> s.copy(list = (if (on) 1 else 0).toUByte()) } }
        button("1.", R.string.format_numbers) { restyleSelection(true, { it.list.toInt() == 2 }) { s, on -> s.copy(list = (if (on) 2 else 0).toUByte()) } }
        button("A", R.string.format_color) {
            val (a, b) = range()
            if (a >= b) return@button
            EditingUi.pickColor(activity, colors, R.string.format_color) { c ->
                setSpans(input.text, restyle(current(), input.text.length, a, b) { it.copy(color = c?.let { v -> (v and 0xFFFFFF).toUInt() }) }, input.textSize / dp)
                input.setSelection(a, b)
            }
        }
        button("⌫", R.string.format_clear) {
            val (a, b) = range()
            setSpans(input.text, restyle(current(), input.text.length, a, b) { RichSpan(0u, 0u, false, false, false, null, 0u, 0u) }, input.textSize / dp)
            input.setSelection(a, b)
        }
        val box = LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            val pad = (12 * dp).toInt()
            setPadding(pad, pad / 2, pad, 0)
            addView(HorizontalScrollView(activity).apply { addView(bar); isHorizontalScrollBarEnabled = false })
            addView(input)
            extra?.invoke(input)?.let { addView(it) }
        }
        val builder = AlertDialog.Builder(activity)
            .setTitle(title)
            .setView(box)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ -> onOk(input.text.toString(), current()) }
        if (compute != null) builder.setNeutralButton(R.string.compute) { _, _ -> }
        val dialog = builder.create()
        dialog.setOnShowListener {
            // The formatting bar comes first; typing must still go to the text.
            input.requestFocus()
            dialog.getButton(AlertDialog.BUTTON_NEUTRAL)?.setOnClickListener {
                val line = input.text.toString().trimEnd().lines().lastOrNull()?.trim().orEmpty()
                if (line.isNotEmpty()) compute?.invoke(line) { result -> input.append("\n= $result"); input.setSelection(input.text.length) }
            }
        }
        dialog.show()
    }

    /** Whether size changes need the block re-measured: always measure with [layout]. */
    fun height(text: String, spans: List<RichSpan>, width: Float, size: Float): Float =
        layout(text, spans, width, size, 0u).height.toFloat()
}


