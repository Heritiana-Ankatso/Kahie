"""Compare two screenshots: the swiped region must have gained dark (ink) pixels."""
import sys
from PIL import Image

before, after = (Image.open(p).convert("L") for p in sys.argv[1:3])
x0, y0, x1, y1 = map(int, sys.argv[3:7])
box = (min(x0, x1), min(y0, y1), max(x0, x1), max(y0, y1))


def dark(img):
    # Anti-aliased thin strokes are mid-grey at the edges: count anything clearly darker than paper.
    return sum(1 for v in img.crop(box).get_flattened_data() if v < 128)


b, a = dark(before), dark(after)
print(f"dark pixels in stroke box: before {b}, after {a}")
# A blank page gives ~0 new dark pixels; two short 3-unit strokes give a few hundred.
if a - b < 100:
    print("no ink appeared where the strokes were drawn")
    sys.exit(1)
print("ink rendered")
