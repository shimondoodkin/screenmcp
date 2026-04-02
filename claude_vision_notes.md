# Claude Vision Image Processing — Coordinate Accuracy

Claude downscales images that exceed ~1568px on any edge before processing them. If an image gets downscaled internally, Claude's coordinate estimates become inaccurate. To get reliable coordinates, **pre-scale images to fit one of these sizes** (the maximum that won't trigger internal resizing):

| Aspect ratio | Image size |
|---|---|
| 1:1 | 1092x1092 px |
| 4:3 | 1268x951 px |
| 3:2 | 1344x896 px |
| 16:9 | 1456x819 px |
| 2:1 | 1568x784 px |

**Only horizontal (landscape) or square aspect ratios work for accurate coordinates.** Vertical images get internally mapped to one of the horizontal tile layouts above, so the image is processed at a different effective resolution than its actual pixel dimensions. This mismatch makes coordinate estimates inaccurate.

## How to pre-scale

1. Pick a horizontal or square size from the table above
2. Scale the image to fit within that size (preserving aspect ratio)
3. Pad with black to fill the exact dimensions
4. Tell Claude the image dimensions so it can estimate coordinates

Token cost formula: `tokens = (width × height) / 750`

A 1092x1092 image uses ~1590 tokens. Smaller images (e.g. 800x800) use fewer tokens with similar accuracy.


Image size	# of Tokens	Cost / image	Cost / 1k images
200x200 px(0.04 megapixels)	~54	~$0.00016	~$0.16
1000x1000 px(1 megapixel)	~1334	~$0.004	~$4.00
1092x1092 px(1.19 megapixels)	~1590	~$0.0048	~$4.80
