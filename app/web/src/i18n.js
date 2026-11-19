// UI strings: French and English (decision D5 recommendation).

const STRINGS = {
  en: {
    library: 'Library', search: 'Search notebooks, pages, text, maths…', newNotebook: 'New notebook',
    newPage: 'New page', import: 'Import', backup: 'Back up everything', untitled: 'Untitled',
    notebook: 'Notebook', myNotebook: 'My notebook', pages: 'pages', pageOne: 'page', page: 'Page', rename: 'Rename',
    delete: 'Delete', duplicate: 'Duplicate', export: 'Export', color: 'Colour', cancel: 'Cancel', ok: 'OK',
    confirmDeleteNotebook: 'Delete the notebook “{0}” and all its pages?', confirmDeletePage: 'Delete this page?',
    noResults: 'Nothing found.', simple: 'Simple', advanced: 'Advanced', mode: 'Mode',
    pen: 'Pen', highlighter: 'Highlighter', eraser: 'Eraser', select: 'Select', text: 'Text', math: 'Maths',
    plot: 'Plot', image: 'Image', shapes: 'Tidy shapes', hand: 'Pan', undo: 'Undo', redo: 'Redo',
    template: 'Paper', size: 'Size', blank: 'Blank', lined: 'Lined', grid: 'Squared', dots: 'Dotted', axes: 'Axes',
    infinite: 'Infinite canvas', exportPng: 'Image (PNG)', exportSvg: 'Vector (SVG)', print: 'Print / PDF',
    exportNotebook: 'Notebook file (.kahie)', printNotebook: 'Print notebook / PDF', importPdf: 'Import PDF to annotate',
    fitPage: 'Fit to screen', more: 'More', back: 'Back', prevPage: 'Previous page', nextPage: 'Next page',
    fingerDraw: 'Draw with finger', mathPrompt: 'LaTeX maths, e.g. \\frac{a}{b} or x^2+1', textPrompt: 'Type text…',
    plotTitle: 'Function plot', plotExprs: 'Functions (one per line), e.g. y = x^2 - 1', range: 'Window',
    showRaw: 'Show my ink', showShape: 'Tidy shape', edit: 'Edit', saved: 'Saved', saving: 'Saving…',
    notSaved: 'Storage blocked: work will not be kept after closing.', importDone: 'Imported {0} notebook(s).',
    importFail: 'This file could not be imported.', pdfLoading: 'Reading PDF… page {0}/{1}', welcome: 'Welcome to Kahie',
    welcomeText: 'Write with your stylus or finger. Tap “Advanced” for maths, plots and shapes.', language: 'Language',
    help: 'Help', helpText: 'Pen: write. Hold still at the end of a stroke to tidy it into a shape. Two fingers: move and zoom. Ctrl+Z: undo.',
    install: 'Install app', clear: 'Clear page', confirmClear: 'Remove everything on this page?', width: 'Width',
    customColor: 'Custom colour', moveUp: 'Move up', moveDown: 'Move down', open: 'Open', addPageAfter: 'Add a page',
  },
  fr: {
    library: 'Bibliothèque', search: 'Chercher carnets, pages, texte, maths…', newNotebook: 'Nouveau carnet',
    newPage: 'Nouvelle page', import: 'Importer', backup: 'Tout sauvegarder', untitled: 'Sans titre',
    notebook: 'Carnet', myNotebook: 'Mon carnet', pages: 'pages', pageOne: 'page', page: 'Page', rename: 'Renommer',
    delete: 'Supprimer', duplicate: 'Dupliquer', export: 'Exporter', color: 'Couleur', cancel: 'Annuler', ok: 'OK',
    confirmDeleteNotebook: 'Supprimer le carnet « {0} » et toutes ses pages ?', confirmDeletePage: 'Supprimer cette page ?',
    noResults: 'Aucun résultat.', simple: 'Simple', advanced: 'Avancé', mode: 'Mode',
    pen: 'Stylo', highlighter: 'Surligneur', eraser: 'Gomme', select: 'Sélection', text: 'Texte', math: 'Maths',
    plot: 'Courbe', image: 'Image', shapes: 'Formes nettes', hand: 'Déplacer', undo: 'Annuler', redo: 'Rétablir',
    template: 'Papier', size: 'Format', blank: 'Blanc', lined: 'Ligné', grid: 'Quadrillé', dots: 'Pointillé', axes: 'Repère',
    infinite: 'Toile infinie', exportPng: 'Image (PNG)', exportSvg: 'Vectoriel (SVG)', print: 'Imprimer / PDF',
    exportNotebook: 'Fichier carnet (.kahie)', printNotebook: 'Imprimer le carnet / PDF', importPdf: 'Importer un PDF à annoter',
    fitPage: 'Ajuster à l’écran', more: 'Plus', back: 'Retour', prevPage: 'Page précédente', nextPage: 'Page suivante',
    fingerDraw: 'Écrire au doigt', mathPrompt: 'Maths LaTeX, ex. \\frac{a}{b} ou x^2+1', textPrompt: 'Écrire du texte…',
    plotTitle: 'Courbe de fonction', plotExprs: 'Fonctions (une par ligne), ex. y = x^2 - 1', range: 'Fenêtre',
    showRaw: 'Voir mon tracé', showShape: 'Forme nette', edit: 'Modifier', saved: 'Enregistré', saving: 'Enregistrement…',
    notSaved: 'Stockage bloqué : le travail ne sera pas conservé après fermeture.', importDone: '{0} carnet(s) importé(s).',
    importFail: 'Ce fichier n’a pas pu être importé.', pdfLoading: 'Lecture du PDF… page {0}/{1}', welcome: 'Bienvenue dans Kahie',
    welcomeText: 'Écrivez au stylet ou au doigt. Touchez « Avancé » pour les maths, courbes et formes.', language: 'Langue',
    help: 'Aide', helpText: 'Stylo : écrire. Restez immobile en fin de trait pour le transformer en forme nette. Deux doigts : déplacer et zoomer. Ctrl+Z : annuler.',
    install: 'Installer l’appli', clear: 'Effacer la page', confirmClear: 'Tout effacer sur cette page ?', width: 'Épaisseur',
    customColor: 'Couleur perso', moveUp: 'Monter', moveDown: 'Descendre', open: 'Ouvrir', addPageAfter: 'Ajouter une page',
  },
};

let lang = 'en';
try { lang = localStorage.getItem('kahie.lang') || ''; } catch {}
if (!STRINGS[lang]) lang = (navigator.language || 'en').toLowerCase().startsWith('fr') ? 'fr' : 'en';

export function t(key, ...args) {
  const s = STRINGS[lang][key] ?? STRINGS.en[key] ?? key;
  return s.replace(/\{(\d)\}/g, (_, i) => args[i] ?? '');
}

export const getLang = () => lang;
export function setLang(l) {
  lang = STRINGS[l] ? l : 'en';
  try { localStorage.setItem('kahie.lang', lang); } catch {}
  document.documentElement.lang = lang;
}
export const LANGS = { en: 'English', fr: 'Français' };
