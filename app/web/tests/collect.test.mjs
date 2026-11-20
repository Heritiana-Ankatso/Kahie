import test from 'node:test';
import assert from 'node:assert/strict';
import { promptsFor, LANGUAGES } from '../src/collect-prompts.js';
import { writerId, shuffled, corpusFile } from '../src/collect.js';

test('prompt ids are unique and stable in shape per language', () => {
  for (const lang of Object.keys(LANGUAGES)) {
    const ps = promptsFor(lang);
    assert.ok(ps.length >= 60, `${lang}: ${ps.length} prompts`);
    assert.equal(new Set(ps.map((p) => p.id)).size, ps.length, `${lang}: duplicate ids`);
    for (const p of ps) {
      assert.match(p.id, /^(fr|en|mg|num|math|shape)-[a-z]?-?\d{4}$|^(fr|en|mg)-[ws]-\d{4}$/);
      assert.ok(['text', 'math', 'shape'].includes(p.kind));
      assert.ok(p.label.length > 0);
    }
  }
});

test('writer ids are random hex with no personal data', () => {
  assert.equal(writerId(() => new Uint8Array([0x3f, 0xa9, 0xc1, 0xd2])), 'w-3fa9c1d2');
  assert.match(writerId((b) => b.fill(7)), /^w-[0-9a-f]{8}$/);
});

test('shuffle is a permutation and does not mutate', () => {
  const a = [1, 2, 3, 4, 5];
  let s = 1;
  const b = shuffled(a, () => ((s = (s * 16807) % 2147483647) / 2147483647));
  assert.deepEqual([...b].sort(), a);
  assert.deepEqual(a, [1, 2, 3, 4, 5]);
});

test('corpus file has the documented shape', () => {
  const f = corpusFile({
    writer: 'w-00000000', consent: { evaluation: true, training: false }, lang: 'mg',
    meta: { handedness: 'left' }, device: { pointer: 'pen' },
    samples: [{ prompt_id: 'mg-w-0000', kind: 'text', label: 'kahie', strokes: [{ points: [[1, 2, 0.5, 0]] }] }],
  });
  assert.equal(f.format, 'kahie-hwr-corpus');
  assert.equal(f.version, 1);
  assert.deepEqual(f.writer, { id: 'w-00000000', handedness: 'left' });
  assert.equal(f.samples[0].strokes[0].points[0].length, 4);
});
