// Prompt sets for the handwriting corpus (docs/spikes/p0-16-corpus-protocol.md).
// Ids are stable: never reuse or renumber an id once samples exist for it.
// The Malagasy lists must be reviewed by a native speaker before a collection session.

const words = {
  fr: ['cahier', 'élève', 'professeur', 'théorème', 'fonction', 'dérivée', 'équation', 'géométrie',
    'triangle', 'hypoténuse', 'probabilité', 'leçon', 'exercice', 'réponse', 'question', 'français',
    'mathématiques', 'année', 'été', 'où', 'déjà', 'garçon', 'cœur', 'naïf', 'Noël'],
  en: ['notebook', 'student', 'teacher', 'theorem', 'function', 'derivative', 'equation', 'geometry',
    'triangle', 'hypotenuse', 'probability', 'lesson', 'exercise', 'answer', 'question', 'English',
    'mathematics', 'year', 'quickly', 'through', 'knowledge', 'rhythm', 'queue', 'jumped', 'wizard'],
  mg: ['kahie', 'mpianatra', 'mpampianatra', 'fampianarana', 'fianarana', 'sekoly', 'boky', 'teny',
    'isa', 'kajy', 'fanontaniana', 'valiny', 'andro', 'volana', 'taona', 'tanàna', 'trano', 'rano',
    'ankizy', 'manoratra', 'mamaky', 'misaotra', 'tsara', 'Madagasikara', 'Antananarivo'],
};

const sentences = {
  fr: ['Le carré de l\'hypoténuse est égal à la somme des carrés des deux autres côtés.',
    'Résoudre l\'équation pour trouver la valeur de x.',
    'La fonction est croissante sur l\'intervalle donné.'],
  en: ['The square of the hypotenuse equals the sum of the squares of the other two sides.',
    'Solve the equation to find the value of x.',
    'The function is increasing on the given interval.'],
  mg: ['Mianatra matematika isan\'andro ny mpianatra.',
    'Soraty ao amin\'ny kahie ny valiny.',
    'Vakio tsara ny fanontaniana.'],
};

const numbers = ['3,14', '2026', '0,5 kg', '12/11/2026', '−7', '1 000 000', '45°', '3 h 15', '25 %', '1,5 m'];

const maths = [
  ['\\frac{a}{b}', 'a/b as a fraction'], ['x^2 + 2x + 1', null], ['\\sqrt{2}', null], ['\\sqrt[3]{x}', null],
  ['\\sum_{k=1}^{n} k', null], ['\\int_0^1 x\\,dx', null], ['\\lim_{x \\to 0} \\frac{\\sin x}{x}', null],
  ['f\'(x) = 3x^2', null], ['a^2 + b^2 = c^2', null], ['\\begin{pmatrix} 1 & 2 \\\\ 3 & 4 \\end{pmatrix}', 'a 2×2 matrix'],
  ['e^{i\\pi} + 1 = 0', null], ['\\Delta = b^2 - 4ac', null], ['x \\in [0, 1]', null], ['u_{n+1} = 2u_n', null],
  ['\\vec{AB} \\cdot \\vec{AC}', null],
];

const shapes = [
  ['line', 'a straight line'], ['circle', 'a circle'], ['ellipse', 'an ellipse'], ['triangle', 'a triangle'],
  ['rectangle', 'a rectangle'], ['square', 'a square'], ['arrow', 'an arrow'], ['parabola', 'a parabola y = x²'],
  ['sine', 'a sine wave'], ['axes', 'x and y axes'],
];

/** All prompts for a language: [{ id, kind, label, hint? }]. */
export function promptsFor(lang) {
  const out = [];
  words[lang].forEach((w, i) => out.push({ id: `${lang}-w-${String(i).padStart(4, '0')}`, kind: 'text', label: w }));
  sentences[lang].forEach((s, i) => out.push({ id: `${lang}-s-${String(i).padStart(4, '0')}`, kind: 'text', label: s }));
  numbers.forEach((n, i) => out.push({ id: `num-${String(i).padStart(4, '0')}`, kind: 'text', label: n }));
  maths.forEach(([tex, hint], i) => out.push({ id: `math-${String(i).padStart(4, '0')}`, kind: 'math', label: tex, hint }));
  shapes.forEach(([s, hint], i) => out.push({ id: `shape-${String(i).padStart(4, '0')}`, kind: 'shape', label: s, hint }));
  return out;
}

export const LANGUAGES = { fr: 'Français', en: 'English', mg: 'Malagasy' };
