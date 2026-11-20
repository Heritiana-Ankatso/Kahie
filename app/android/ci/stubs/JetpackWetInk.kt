// Compile-check stub only (app/android/ci/kt-check.sh): androidx.ink is not on Maven Central.
package app.kahie
import android.content.Context
import android.view.MotionEvent
import android.view.View
class JetpackWetInk(context: Context) : InkView.WetInk {
    val view: View = View(context)
    override fun start(e: MotionEvent, pointerId: Int, highlighter: Boolean, colorArgb: Int, sizePx: Float) = true
    override fun add(e: MotionEvent, pointerId: Int) {}
    override fun finish(e: MotionEvent, pointerId: Int) {}
    override fun cancel(e: MotionEvent?) {}
}
