package app.kahie

import android.app.Activity
import android.app.AlertDialog
import android.content.SharedPreferences
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.view.View
import android.widget.CheckBox
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.RadioButton
import android.widget.RadioGroup
import android.widget.TextView
import uniffi.nb_ffi.InstrumentKind
import uniffi.nb_ffi.PageDoc
import uniffi.nb_ffi.TidyMode

/**
 * Menus for the drawing aids: shape tidying modes (E2.8), snapping (E2.9) and instruments
 * (E2.10); the "Tidy" chip shown after a stroke with a suggested shape; selection entries to
 * switch between a tidied shape and the original ink.
 */
class DrawingAidsUi(
    private val activity: Activity,
    private val ink: InkView,
    private val prefs: SharedPreferences,
    private val doc: () -> PageDoc?,
    private val changed: () -> Unit,
) {
    private val dp = activity.resources.displayMetrics.density
    private var chip: TextView? = null
    private val hideChip = Runnable { chip?.visibility = View.GONE }

    /** Restore the saved settings (older versions only had "automatic shapes"). */
    fun load() {
        val legacy = if (prefs.getBoolean("autoShapes", false)) "auto" else "free"
        ink.tidyMode = when (prefs.getString(PREF_TIDY, legacy)) {
            "suggest" -> TidyMode.SUGGEST
            "auto" -> TidyMode.AUTO
            else -> TidyMode.FREE
        }
        ink.snapping = prefs.getBoolean(PREF_SNAP, true)
        ink.onShapeSuggested = { showChip(it) }
    }

    /** The chip floats over the bottom of the page. */
    fun attach(page: FrameLayout) {
        val c = TextView(activity).apply {
            text = activity.getString(R.string.tidy_this)
            setTextColor(0xFFFFFFFF.toInt())
            textSize = 15f
            gravity = Gravity.CENTER
            minHeight = (44 * dp).toInt()
            setPadding((20 * dp).toInt(), 0, (20 * dp).toInt(), 0)
            background = GradientDrawable().apply { cornerRadius = 22 * dp; setColor(0xE62B59C3.toInt()) }
            visibility = View.GONE
        }
        page.addView(c, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT,
            Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL).apply { bottomMargin = (88 * dp).toInt() })
        chip = c
    }

    private fun showChip(id: String) {
        val c = chip ?: return
        c.setOnClickListener {
            if (doc()?.setShapesShown(listOf(id), true) == true) changed()
            c.visibility = View.GONE
        }
        c.visibility = View.VISIBLE
        c.removeCallbacks(hideChip)
        c.postDelayed(hideChip, 4000)
    }

    /** Free / Suggest / Auto, and snapping. */
    fun showShapeOptions(onDone: () -> Unit) {
        val box = LinearLayout(activity).apply {
            orientation = LinearLayout.VERTICAL
            setPadding((20 * dp).toInt(), (8 * dp).toInt(), (20 * dp).toInt(), 0)
        }
        val modes = listOf(TidyMode.FREE to R.string.tidy_free, TidyMode.SUGGEST to R.string.tidy_suggest, TidyMode.AUTO to R.string.tidy_auto)
        val group = RadioGroup(activity)
        modes.forEachIndexed { i, (m, label) ->
            group.addView(RadioButton(activity).apply {
                id = 1000 + i
                text = activity.getString(label)
                isChecked = ink.tidyMode == m
                minHeight = (44 * dp).toInt()
            })
        }
        val snap = CheckBox(activity).apply {
            text = activity.getString(R.string.snap_toggle)
            isChecked = ink.snapping
            minHeight = (44 * dp).toInt()
        }
        box.addView(group)
        box.addView(snap)
        AlertDialog.Builder(activity)
            .setTitle(R.string.shapes_title)
            .setView(box)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val m = modes.getOrNull(group.checkedRadioButtonId - 1000)?.first ?: ink.tidyMode
                ink.tidyMode = m
                ink.snapping = snap.isChecked
                prefs.edit()
                    .putString(PREF_TIDY, when (m) { TidyMode.SUGGEST -> "suggest"; TidyMode.AUTO -> "auto"; else -> "free" })
                    .putBoolean(PREF_SNAP, snap.isChecked)
                    .apply()
                onDone()
            }
            .show()
    }

    /** Ruler, compass, protractor, or put the instrument away. */
    fun showInstruments() {
        val kinds = listOf(InstrumentKind.RULER to R.string.instrument_ruler, InstrumentKind.COMPASS to R.string.instrument_compass,
            InstrumentKind.PROTRACTOR to R.string.instrument_protractor)
        val labels = kinds.map { activity.getString(it.second) } +
            (if (ink.instruments.spec != null) listOf(activity.getString(R.string.instrument_none)) else emptyList())
        AlertDialog.Builder(activity)
            .setTitle(R.string.instruments)
            .setItems(labels.toTypedArray()) { _, i ->
                val c = ink.visibleCentre()
                ink.instruments.choose(kinds.getOrNull(i)?.first, c[0], c[1], ink.visibleWidth())
                ink.invalidate()
            }
            .show()
    }

    /** Selection menu entries: show the tidied shape or the original ink. */
    fun selectionExtras(ids: List<String>): List<Pair<String, () -> Unit>> {
        val d = doc() ?: return emptyList()
        return when (d.shapesShown(ids)) {
            true -> listOf(activity.getString(R.string.show_original_ink) to { if (d.setShapesShown(ids, false)) changed() })
            false -> listOf(activity.getString(R.string.show_tidy_shape) to { if (d.setShapesShown(ids, true)) changed() })
            null -> emptyList()
        }
    }

    companion object {
        const val PREF_TIDY = "tidyMode"
        const val PREF_SNAP = "snapping"
    }
}
