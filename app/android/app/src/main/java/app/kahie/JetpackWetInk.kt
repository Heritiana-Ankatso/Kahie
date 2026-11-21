package app.kahie

import android.content.Context
import android.view.MotionEvent
import android.view.View
import androidx.ink.authoring.InProgressStrokeId
import androidx.ink.authoring.InProgressStrokesFinishedListener
import androidx.ink.authoring.InProgressStrokesView
import androidx.ink.brush.Brush
import androidx.ink.brush.StockBrushes
import androidx.ink.strokes.Stroke

/**
 * Low-latency wet ink drawn by Jetpack Ink (front-buffered rendering).
 *
 * Prototype. The Rust core stays the source of truth: [InkView] still sends every
 * point to the core and draws the committed geometry. This layer only draws the
 * stroke in progress, and drops its copy once the stroke is finished, so the
 * core's rendering replaces it.
 */
class JetpackWetInk(context: Context) : InkView.WetInk {
    private val strokes = InProgressStrokesView(context).apply {
        // Overlay only: let touches fall through to the InkView underneath.
        isClickable = false
        isFocusable = false
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        addFinishedStrokesListener(object : InProgressStrokesFinishedListener {
            override fun onStrokesFinished(strokes: Map<InProgressStrokeId, Stroke>) {
                // InkView has already refreshed from the core by now.
                removeFinishedStrokes(strokes.keys)
            }
        })
    }
    private var current: InProgressStrokeId? = null

    /** The view to stack exactly on top of the InkView. */
    val view: View get() = strokes

    override fun start(e: MotionEvent, pointerId: Int, highlighter: Boolean, colorArgb: Int, sizePx: Float): Boolean {
        val family = if (highlighter) StockBrushes.highlighter() else StockBrushes.pressurePen()
        val brush = Brush.createWithColorIntArgb(family, colorArgb, sizePx, 0.1f)
        current = strokes.startStroke(e, pointerId, brush)
        return true
    }

    override fun add(e: MotionEvent, pointerId: Int) {
        val id = current ?: return
        strokes.addToStroke(e, pointerId, id, null)
    }

    override fun finish(e: MotionEvent, pointerId: Int) {
        val id = current ?: return
        strokes.finishStroke(e, pointerId, id)
        current = null
    }

    override fun cancel(e: MotionEvent?) {
        val id = current ?: return
        strokes.cancelStroke(id, e)
        current = null
    }
}
