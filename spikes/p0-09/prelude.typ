// Definitions for the helper functions emitted by MiTeX's LaTeX → Typst conversion.
// Written for Kahie (GPL-3.0-or-later); covers the commands used by the reference set.
#let zws = sym.zws
#let mitexsqrt(..args) = {
  let a = args.pos()
  if a.len() == 1 { math.sqrt(a.at(0)) } else { math.root(a.at(0), a.at(1)) }
}
#let mitexoverbrace(it) = math.overbrace(it)
#let mitexunderbrace(it) = math.underbrace(it)
#let textmath(it) = text(it)
#let operatorname(it) = math.op(math.upright(it))
#let boxed(it) = box(stroke: 0.6pt, inset: (x: 3pt, y: 2pt), baseline: 25%, $it$)
#let pmatrix(..a) = math.mat(delim: "(", ..a)
#let bmatrix(..a) = math.mat(delim: "[", ..a)
#let vmatrix(..a) = math.mat(delim: "|", ..a)
#let Bmatrix(..a) = math.mat(delim: "{", ..a)
#let Vmatrix(..a) = math.mat(delim: "‖", ..a)
#let matrix(..a) = math.mat(delim: none, ..a)
#let aligned(..a) = math.mat(delim: none, align: left, ..a)
// Symbols renamed in Typst 0.14+ that MiTeX 0.2 still emits.
#let diff = sym.partial
#let sect = sym.inter
#let pmod(it) = $quad (mod it)$
