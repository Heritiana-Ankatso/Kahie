package app.kahie

import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import uniffi.nb_ffi.InstrumentKind
import uniffi.nb_ffi.InstrumentSpec
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.roundToInt
import kotlin.math.sin

/**
 * Virtual instruments (E2.10) lying on the page: ruler, compass and protractor. The geometry
 * that guides the pen and the readings are in the core (`nb_recog::instruments`); this class
 * draws the instrument and lets a finger move, turn or open it. Everything is in page units.
 *
 * - Ruler: drag its body to move it; drag the round handle at its far end to turn it.
 * - Compass: drag the centre pin to move it; drag the pencil handle to open and turn it.
 * - Protractor: drag its body to move it; drag the handle at the right of its base to turn it.
 *
 * A stroke that starts on the ruler's edge, on the compass circle or at the protractor's
 * centre follows the instrument.
 */
class InstrumentOverlay(private val density: Float) {
    var spec: InstrumentSpec? = null

    /** Ruler body width, protractor default radius (page units). */
    private val rulerWidth = 64f
    private var drag = 0 // 0 none, 1 move, 2 handle
    private var lastX = 0f
    private var lastY = 0f
    private var grabAngle = 0.0
    private var grabSpecAngle = 0.0

    val dragging get() = drag != 0

    private val body = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0x332B59C3; style = Paint.Style.FILL }
    private val edge = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xCC2B59C3.toInt(); style = Paint.Style.STROKE }
    private val tick = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xCC1D1D1F.toInt(); style = Paint.Style.STROKE }
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF1D1D1F.toInt(); textAlign = Paint.Align.CENTER }
    private val handle = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF2B59C3.toInt(); style = Paint.Style.FILL }
    private val label = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE; textAlign = Paint.Align.CENTER }
    private val labelBg = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xE61D1D1F.toInt() }

    /** Put an instrument of this kind in the middle of the visible area (`null` removes it). */
    fun choose(kind: InstrumentKind?, cx: Float, cy: Float, viewWidth: Float) {
        spec = when (kind) {
            null -> null
            InstrumentKind.RULER -> {
                val len = (viewWidth * 0.8f).coerceIn(200f, 1200f)
                InstrumentSpec(kind, (cx - len / 2).toDouble(), cy.toDouble(), 0.0, len.toDouble(), 0.0)
            }
            InstrumentKind.COMPASS -> InstrumentSpec(kind, cx.toDouble(), cy.toDouble(), 0.0, 3.0 * UNITS_PER_CM, 0.0)
            InstrumentKind.PROTRACTOR -> InstrumentSpec(kind, cx.toDouble(), (cy + 80f).toDouble(), 0.0, 180.0, 0.0)
        }
    }

    /** The spec handed to the core for a stroke, with a capture distance of ~16 dp. */
    fun forStroke(scale: Float): InstrumentSpec? = spec?.copy(capture = (16f * density / scale).toDouble())

    private fun handlePoint(sp: InstrumentSpec): Pair<Float, Float> {
        val a = sp.angle
        return when (sp.kind) {
            InstrumentKind.RULER -> {
                // Middle of the far end of the body (body on the +normal side).
                val ex = sp.x + sp.size * cos(a) - sin(a) * rulerWidth / 2
                val ey = sp.y + sp.size * sin(a) + cos(a) * rulerWidth / 2
                ex.toFloat() to ey.toFloat()
            }
            InstrumentKind.COMPASS -> (sp.x + sp.size * cos(a)).toFloat() to (sp.y + sp.size * sin(a)).toFloat()
            InstrumentKind.PROTRACTOR -> (sp.x + (sp.size + 24) * cos(a)).toFloat() to (sp.y + (sp.size + 24) * sin(a)).toFloat()
        }
    }

    /** Whether the page point is on the instrument's body (where a finger moves it). */
    private fun onBody(sp: InstrumentSpec, x: Float, y: Float, tol: Float): Boolean {
        val dx = x - sp.x
        val dy = y - sp.y
        val u = dx * cos(sp.angle) + dy * sin(sp.angle)
        val v = -dx * sin(sp.angle) + dy * cos(sp.angle)
        return when (sp.kind) {
            InstrumentKind.RULER -> u in -tol.toDouble()..(sp.size + tol) && v in 0.0..(rulerWidth + tol).toDouble()
            InstrumentKind.COMPASS -> hypot(dx, dy) <= max(tol * 1.5f, 20f)
            InstrumentKind.PROTRACTOR -> hypot(dx, dy) <= sp.size + tol && v <= tol
        }
    }

    /**
     * A finger touch at page point (x, y); returns true if the instrument takes it (the
     * gesture then belongs to the instrument until the finger lifts).
     */
    fun touch(action: Int, x: Float, y: Float, scale: Float): Boolean {
        val sp = spec ?: return false
        val tol = 18f * density / scale
        when (action) {
            android.view.MotionEvent.ACTION_DOWN -> {
                val (hx, hy) = handlePoint(sp)
                drag = when {
                    hypot(x - hx, y - hy) <= tol * 1.4f -> 2
                    onBody(sp, x, y, tol) -> 1
                    else -> 0
                }
                lastX = x; lastY = y
                grabAngle = atan2((y - sp.y), (x - sp.x).toDouble())
                grabSpecAngle = sp.angle
                return drag != 0
            }
            android.view.MotionEvent.ACTION_MOVE -> {
                if (drag == 0) return false
                spec = if (drag == 1) {
                    sp.copy(x = sp.x + (x - lastX), y = sp.y + (y - lastY))
                } else {
                    val ang = atan2((y - sp.y), (x - sp.x).toDouble())
                    when (sp.kind) {
                        // Turning snaps to whole degrees.
                        InstrumentKind.RULER, InstrumentKind.PROTRACTOR -> sp.copy(angle = roundDeg(grabSpecAngle + ang - grabAngle))
                        InstrumentKind.COMPASS -> sp.copy(angle = ang, size = hypot(x - sp.x, y - sp.y).coerceAtLeast(8.0))
                    }
                }
                lastX = x; lastY = y
                return true
            }
            android.view.MotionEvent.ACTION_UP, android.view.MotionEvent.ACTION_CANCEL -> {
                val was = drag != 0
                drag = 0
                return was
            }
        }
        return drag != 0
    }

    fun cancelDrag() { drag = 0 }

    private fun roundDeg(a: Double): Double = Math.toRadians(Math.toDegrees(a).roundToInt().toDouble())

    /** Draw the instrument (canvas in page units, `scale` = screen pixels per page unit). */
    fun draw(canvas: Canvas, scale: Float) {
        val sp = spec ?: return
        val px = 1f / scale
        edge.strokeWidth = 1.5f * density * px
        tick.strokeWidth = 1f * density * px
        text.textSize = 10f * density * px
        canvas.save()
        canvas.translate(sp.x.toFloat(), sp.y.toFloat())
        canvas.rotate(Math.toDegrees(sp.angle).toFloat())
        val cm = UNITS_PER_CM.toFloat()
        when (sp.kind) {
            InstrumentKind.RULER -> {
                val len = sp.size.toFloat()
                canvas.drawRect(0f, 0f, len, rulerWidth, body)
                canvas.drawLine(0f, 0f, len, 0f, edge)
                var i = 0
                while (i * cm / 2 <= len) {
                    val major = i % 2 == 0
                    canvas.drawLine(i * cm / 2, 0f, i * cm / 2, if (major) 14f else 8f, tick)
                    if (major && cm * scale > 18 * density) canvas.drawText("${i / 2}", i * cm / 2, 26f, text)
                    i++
                }
            }
            InstrumentKind.COMPASS -> {
                val r = sp.size.toFloat()
                edge.pathEffect = android.graphics.DashPathEffect(floatArrayOf(6f * px * density, 6f * px * density), 0f)
                canvas.drawCircle(0f, 0f, r, edge)
                edge.pathEffect = null
                canvas.drawLine(0f, 0f, r, 0f, edge)
                canvas.drawCircle(0f, 0f, 5f * density * px, handle)
                canvas.drawText("r = %.1f cm".format(sp.size / UNITS_PER_CM), r / 2, -6f * density * px, text)
            }
            InstrumentKind.PROTRACTOR -> {
                val r = sp.size.toFloat()
                val oval = RectF(-r, -r, r, r)
                canvas.drawArc(oval, 180f, 180f, true, body)
                canvas.drawArc(oval, 180f, 180f, false, edge)
                canvas.drawLine(-r, 0f, r, 0f, edge)
                for (d in 0..180) {
                    val len = when { d % 10 == 0 -> 16f; d % 5 == 0 -> 10f; else -> 5f }
                    if (d % 5 != 0 && r * scale < 140 * density) continue
                    val a = Math.toRadians(d.toDouble())
                    val c = cos(a).toFloat(); val s = -sin(a).toFloat()
                    canvas.drawLine(r * c, r * s, (r - len) * c, (r - len) * s, tick)
                    if (d % 30 == 0) canvas.drawText("$d", (r - 28f) * c, (r - 28f) * s + 4f, text)
                }
                canvas.drawCircle(0f, 0f, 3f * density * px, handle)
            }
        }
        canvas.restore()
        val (hx, hy) = handlePoint(sp)
        canvas.drawCircle(hx, hy, 9f * density * px, handle)
    }

    /**
     * A reading next to the pen for a guided stroke from (x0, y0) to (x, y): the length drawn
     * along the ruler, the radius on the compass, the angle on the protractor.
     */
    fun drawReading(canvas: Canvas, scale: Float, value: String, x: Float, y: Float) {
        val px = 1f / scale
        label.textSize = 13f * density * px
        val w = label.measureText(value) + 12f * density * px
        val h = 20f * density * px
        val top = y - 34f * density * px
        canvas.drawRoundRect(x - w / 2, top, x + w / 2, top + h, 6f * density * px, 6f * density * px, labelBg)
        canvas.drawText(value, x, top + h - 6f * density * px, label)
    }

    fun readingText(sp: InstrumentSpec, startReading: Double, reading: Double): String = when (sp.kind) {
        InstrumentKind.RULER -> "%.1f cm".format(kotlin.math.abs(reading - startReading))
        InstrumentKind.COMPASS -> "r = %.1f cm".format(sp.size / UNITS_PER_CM)
        InstrumentKind.PROTRACTOR -> "${reading.roundToInt()}°"
    }

    companion object {
        /** Page units per centimetre (A4: 794 units for 21 cm), as in the core. */
        const val UNITS_PER_CM = 794.0 / 21.0
    }
}
