package app.kahie

import android.content.Context
import uniffi.nb_ffi.NotebookDoc
import uniffi.nb_ffi.OutlineKind
import uniffi.nb_ffi.PageDoc
import java.io.File

/**
 * Page templates (P1-08): a built-in set made in code (headings in the app's language) and
 * the teacher's own, saved as page files in `files/templates/`. A new page is made from a
 * template with `PageDoc.copyContentFrom`.
 */
object Templates {
    /** Built-in template ids, in menu order. */
    val builtIn = listOf("lesson", "lecture", "exercise", "research", "journal", "planner")

    fun title(ctx: Context, id: String): String = ctx.getString(
        when (id) {
            "lesson" -> R.string.tpl_lesson
            "lecture" -> R.string.tpl_lecture
            "exercise" -> R.string.tpl_exercise
            "research" -> R.string.tpl_research
            "journal" -> R.string.tpl_journal
            else -> R.string.tpl_planner
        },
    )

    fun dir(ctx: Context) = File(ctx.filesDir, "templates").apply { mkdirs() }

    /** Saved templates: display name → page file. */
    fun saved(ctx: Context): List<Pair<String, File>> =
        dir(ctx).listFiles { f -> f.name.endsWith(".kpage") }?.sortedBy { it.name }
            ?.map { it.name.removeSuffix(".kpage") to it } ?: emptyList()

    /** Save a copy of `page` as a template named `name` (replacing one with that name). */
    fun save(ctx: Context, page: PageDoc, name: String) {
        val safe = name.replace(Regex("[\\\\/:*?\"<>|]"), "_").take(60).ifBlank { "template" }
        val f = File(dir(ctx), "$safe.kpage")
        listOf(f, File(f.path + "-wal"), File(f.path + "-shm")).forEach { it.delete() }
        PageDoc.open(f.path).apply {
            copyContentFrom(page)
            save()
        }
    }

    /**
     * A built-in template as an in-memory page. `measure(text, width, size)` gives the
     * height of a text block (the app's text layout), so blocks are sized like typed ones.
     */
    fun build(ctx: Context, id: String, measure: (String, Float, Float) -> Float): PageDoc {
        val p = PageDoc.newInMemory()
        val ink = 0x1D1D1Fu
        val accent = 0x2B59C3u
        fun text(x: Float, y: Float, w: Float, s: String, size: Float, color: UInt = ink) {
            p.addText(x.toDouble(), y.toDouble(), w.toDouble(), measure(s, w, size).toDouble(), s, size.toDouble(), color)
        }
        fun s(res: Int) = ctx.getString(res)
        when (id) {
            "lesson" -> {
                p.setTemplate("lined")
                text(100f, 24f, 640f, s(R.string.tpl_lesson), 28f, accent)
                text(100f, 64f, 640f, s(R.string.tpl_class_date_duration), 16f)
                val heads = listOf(R.string.tpl_objectives, R.string.tpl_prerequisites, R.string.tpl_materials,
                    R.string.tpl_introduction, R.string.tpl_main_activity, R.string.tpl_practice,
                    R.string.tpl_assessment, R.string.tpl_homework)
                heads.forEachIndexed { i, h -> text(100f, 128f + i * 120f, 640f, s(h), 18f, accent) }
            }
            "lecture" -> {
                p.setTemplate("cornell")
                text(230f, 24f, 540f, s(R.string.tpl_lecture), 26f, accent)
                text(230f, 60f, 540f, s(R.string.tpl_course_date), 15f)
                text(16f, 24f, 190f, s(R.string.tpl_cues), 16f, accent)
                text(16f, 1123f - 205f, 760f, s(R.string.tpl_summary), 16f, accent)
            }
            "exercise" -> {
                p.setTemplate("grid")
                text(48f, 24f, 700f, s(R.string.tpl_name_class_date), 16f)
                text(48f, 64f, 700f, s(R.string.tpl_exercise), 26f, accent)
                for (i in 1..3) text(48f, 128f + (i - 1) * 320f, 700f, s(R.string.tpl_exercise_n).format(i), 18f, accent)
            }
            "research" -> {
                p.setPageSize("a4-landscape")
                p.setTemplate("dots")
                text(40f, 20f, 1040f, s(R.string.tpl_research), 26f, accent)
                val heads = listOf(R.string.tpl_question, R.string.tpl_hypotheses, R.string.tpl_sources, R.string.tpl_findings)
                heads.forEachIndexed { i, h -> text(40f + (i % 2) * 540f, 80f + (i / 2) * 360f, 500f, s(h), 18f, accent) }
            }
            "journal" -> {
                p.setTemplate("lined")
                text(100f, 24f, 640f, s(R.string.tpl_journal_date), 20f, accent)
                text(100f, 128f, 640f, s(R.string.tpl_today), 18f, accent)
                text(100f, 512f, 640f, s(R.string.tpl_went_well), 18f, accent)
                text(100f, 800f, 640f, s(R.string.tpl_to_improve), 18f, accent)
            }
            else -> { // weekly planner: one column per day
                p.setPageSize("a4-landscape")
                p.setTemplate("grid")
                text(24f, 16f, 1070f, s(R.string.tpl_week_of), 22f, accent)
                val locale = ctx.resources.configuration.locales[0]
                java.time.DayOfWeek.entries.forEachIndexed { i, day ->
                    val name = day.getDisplayName(java.time.format.TextStyle.FULL, locale)
                        .replaceFirstChar { it.titlecase(locale) }
                    text(24f + i * 154f, 64f, 146f, name, 15f, accent)
                }
            }
        }
        return p
    }

    // ---------- notebook templates (P1-08) ----------

    /** Built-in notebook template ids, in menu order. */
    val builtInNotebooks = listOf("course", "planner", "exercises")

    fun notebookTitle(ctx: Context, id: String): String = ctx.getString(
        when (id) {
            "course" -> R.string.nbtpl_course
            "planner" -> R.string.nbtpl_planner
            else -> R.string.nbtpl_exercises
        },
    )

    fun notebookDir(ctx: Context) = File(ctx.filesDir, "notebook-templates").apply { mkdirs() }

    /** Saved notebook templates: display name → notebook directory. */
    fun savedNotebooks(ctx: Context): List<Pair<String, File>> =
        notebookDir(ctx).listFiles { f -> f.isDirectory }?.sortedBy { it.name }?.map { it.name to it } ?: emptyList()

    /** Save the structure and pages of `notebook` as a template named `name` (replacing one). */
    fun saveNotebook(ctx: Context, notebook: NotebookDoc, name: String) {
        val safe = name.replace(Regex("[\\\\/:*?\"<>|]"), "_").take(60).ifBlank { "template" }
        val dir = File(notebookDir(ctx), safe)
        dir.deleteRecursively()
        NotebookDoc.open(dir.path, safe).apply {
            copyFromTemplate(notebook)
            save()
        }
    }

    /**
     * A built-in notebook template, built in a scratch directory: sections of pages, each
     * page made from a built-in page template (headings in the app's language).
     */
    fun buildNotebook(ctx: Context, id: String, measure: (String, Float, Float) -> Float): NotebookDoc {
        fun s(res: Int) = ctx.getString(res)
        // (section title or null for top level) to (page template, page title)
        val plan: List<Pair<String?, List<Pair<String, String>>>> = when (id) {
            "course" -> listOf(
                s(R.string.nbtpl_sec_lessons) to (1..3).map { "lecture" to "" },
                s(R.string.nbtpl_sec_exercises) to (1..3).map { "exercise" to "" },
                s(R.string.nbtpl_sec_assessments) to listOf("exercise" to ""),
            )
            "planner" -> listOf(
                null to (1..4).map { "planner" to s(R.string.nbtpl_week_n).format(it) } + listOf("journal" to ""),
            )
            else -> listOf(null to (1..5).map { "exercise" to "" })
        }
        val dir = File(ctx.cacheDir, "nbtpl-$id").apply { deleteRecursively() }
        val nb = NotebookDoc.open(dir.path, notebookTitle(ctx, id))
        val initial = nb.outline().first { it.kind == OutlineKind.PAGE }.node
        val pages = HashMap<String, PageDoc>()
        for ((section, entries) in plan) {
            val parent = section?.let { nb.addSection(it) }
            entries.forEachIndexed { i, (tpl, title) ->
                val pid = nb.addPage(null)
                val node = nb.outline().first { it.pageId == pid }.node
                if (parent != null) nb.moveNode(node, parent, i.toUInt())
                if (title.isNotEmpty()) nb.rename(node, title)
                val src = pages.getOrPut(tpl) { build(ctx, tpl, measure) }
                PageDoc.open(nb.pagePath(pid)).apply {
                    copyContentFrom(src)
                    save()
                }
            }
        }
        nb.deleteNode(initial)
        nb.emptyTrash()
        nb.save()
        return nb
    }
}
