package app.kahie

import android.content.Context
import android.graphics.drawable.PictureDrawable
import android.os.Handler
import android.os.Looper
import android.text.Editable
import android.text.TextWatcher
import android.view.View
import android.widget.Button
import android.widget.EditText
import android.widget.HorizontalScrollView
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView

/**
 * Maths editor (E2.1, first step): the LaTeX field with a live preview and palettes of
 * structures and symbols. A structure is inserted at the cursor, which lands in its first
 * empty slot (e.g. the numerator of \frac{}{}).
 */
object MathEditor {
    /** Label shown on the button → LaTeX inserted ("{}" slots; the cursor goes to the first). */
    private val structures = listOf(
        "a/b" to "\\frac{}{}", "√" to "\\sqrt{}", "ⁿ√" to "\\sqrt[]{}", "xⁿ" to "^{}", "xₙ" to "_{}",
        "( )" to "\\left(  \\right)", "|x|" to "\\left|  \\right|", "∫" to "\\int_{}^{} \\, dx", "Σ" to "\\sum_{}^{}",
        "lim" to "\\lim_{x \\to }", "→v" to "\\vec{}", "x̄" to "\\overline{}", "{ ," to "\\begin{cases}  \\\\  \\end{cases}",
        "( ⋮ )" to "\\begin{pmatrix}  &  \\\\  &  \\end{pmatrix}",
    )
    private val symbols = listOf(
        "±" to "\\pm ", "×" to "\\times ", "·" to "\\cdot ", "÷" to "\\div ", "≤" to "\\le ", "≥" to "\\ge ", "≠" to "\\ne ",
        "≈" to "\\approx ", "∞" to "\\infty ", "∈" to "\\in ", "ℝ" to "\\mathbb{R}", "⇒" to "\\Rightarrow ", "⇔" to "\\Leftrightarrow ",
        "°" to "^{\\circ}", "∠" to "\\widehat{}", "π" to "\\pi ", "α" to "\\alpha ", "β" to "\\beta ", "θ" to "\\theta ",
        "λ" to "\\lambda ", "Δ" to "\\Delta ", "ω" to "\\omega ",
    )

    fun view(ctx: Context, input: EditText, renderer: MathRenderer, size: Float, color: Int): View {
        val density = ctx.resources.displayMetrics.density
        val pad = (16 * density).toInt()
        val preview = ImageView(ctx).apply {
            minimumHeight = (56 * density).toInt()
            adjustViewBounds = true
            scaleType = ImageView.ScaleType.FIT_START
            contentDescription = ctx.getString(R.string.math_preview)
        }
        val status = TextView(ctx).apply { textSize = 12f }
        val main = Handler(Looper.getMainLooper())
        var generation = 0
        fun update() {
            val latex = input.text.toString().trim()
            val g = ++generation
            if (latex.isEmpty()) { preview.setImageDrawable(null); status.text = ""; return }
            renderer.request(latex, size, color) { r ->
                if (g != generation) return@request
                if (r.error || r.picture == null) {
                    status.text = ctx.getString(R.string.math_error)
                } else {
                    status.text = ""
                    preview.setImageDrawable(PictureDrawable(r.picture))
                }
            }
        }
        val debounce = Runnable { update() }
        input.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) {
                main.removeCallbacks(debounce)
                main.postDelayed(debounce, 300)
            }
        })
        fun row(items: List<Pair<String, String>>) = HorizontalScrollView(ctx).apply {
            val line = LinearLayout(ctx)
            for ((label, latex) in items) {
                line.addView(Button(ctx, null, android.R.attr.buttonStyleSmall).apply {
                    text = label
                    isAllCaps = false
                    contentDescription = latex.trim()
                    setOnClickListener { insert(input, latex) }
                })
            }
            addView(line)
        }
        update()
        // Typing goes to the LaTeX field, not to the palettes.
        input.post { input.requestFocus() }
        return LinearLayout(ctx).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(pad, pad / 2, pad, 0)
            addView(preview)
            addView(status)
            addView(input)
            addView(row(structures))
            addView(row(symbols))
        }
    }

    /** Insert `latex` at the cursor (replacing a selection) and put the cursor in its first slot. */
    private fun insert(input: EditText, latex: String) {
        val start = minOf(input.selectionStart, input.selectionEnd).coerceAtLeast(0)
        val end = maxOf(input.selectionStart, input.selectionEnd).coerceAtLeast(0)
        input.text.replace(start, end, latex)
        val slot = latex.indexOf("{}").takeIf { it >= 0 }?.plus(1)
            ?: latex.indexOf("[]").takeIf { it >= 0 }?.plus(1)
            ?: latex.indexOf("  ").takeIf { it >= 0 }?.plus(1)
            ?: latex.length
        input.setSelection((start + slot).coerceAtMost(input.text.length))
    }
}
