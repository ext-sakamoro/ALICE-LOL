# Sheet law `avatar_sheet`: how to fill a character sheet set

law id `1f85ea76e2d64c11f86600caa1c64ad2f14a6be41c0b35db0865725605ec7dbe`

## What this is

- a sheet set is several pages: the turnaround, the base body, the face, the hair, each clothing layer, accessories and footwear, the materials and the palette; each page is drawn from these slots and the avatar laws, never from a free drawing
- lengths are in the units of the avatar laws (the body: the eye height above the sole is 1; the face: the chin-to-eye distance is 1) unless a slot names another unit
- a slot is filled with a value of its type inside its range or vocabulary; a required slot left empty, a value out of range, a reference to nothing or a file without provenance is listed by the check, and no page is drawn until every required slot of the set is filled
- pending marks a slot that exists here before the part law that should own it as a knob has it

## How to answer

Answer with one JSON object and nothing else: `{"sheet_fill": 1, "law": "avatar_sheet", "pages": { … }}`. Under `pages`, each page is an object; a slot is a key with its value; a group is a key with a list of items, each item an object of its fields. Use only the keys listed below. Numbers are plain JSON numbers in the slot's unit. Text has no escapes (no `\\` and no `\"`). A value you cannot decide is `null`: it will be listed as missing, which is better than a guess.

Families: `base_body.family` chooses the laws the ranges come from, and a range column lists each family's range. Choose the family from the idea, then use that family's whole range.
- `reference`: `body` = `avatar_body`, `face` = `avatar_face`
- `stylised`: `body` = `avatar_body_stylised`, `face` = `avatar_face_stylised`

Units: `eye_height` = the height of the eyes above the sole is 1; `chin_eye` = the distance from the chin to the eye line is 1; `fraction` = 0 to 1; `m`, `mm`, `deg`, `ratio` as usual.

A file (`asset`) is an object: `{"file": "name.png", "format": "png", "width": 1024, "height": 1024, "colour_space": "srgb", "provenance": {"source": "…", "licence": "…", "author": "…", "terms_sha256": "<64 hex digits of the licence text>"}}`; formats: png, jpg, webp, svg, exr, tga.

Derive every value from the idea and use the whole range: the middle of a range is not a target. When the design wants a number outside a slot's range, put the nearest value inside the range in the slot and add a note at the top level: `"out_of_range_intent": [{"slot": "base_body.knee", "wants": 0.27, "why": "…"}]` (the reason at most 120 characters). The note is listed as a warning, not an error, and tells where the ranges are too narrow.

Complete means: every required slot (and every field of every item, when its condition holds) has a value of its type inside its range or vocabulary, every name you refer to exists, every rule holds, and every file has its provenance.

## Page `turnaround`

the body from the front, the side and the back at one scale, standing on one ground line, with guide lines at the landmark heights and the joints marked

| slot | type | unit | range | need | meaning |
|---|---|---|---|---|---|
| `stature` | number | m | 0.5 – 2.5 | required | height from the sole to the top of the skull, without hair |
| `pose` | one of T / A |  |  | required | arms straight out (T) or lowered (A) |
| `arm_angle` | number | deg | 0 – 60 | required when pose = A | angle of the arms below the horizontal |
| `views` | one of front_side_back |  |  | required | the three views drawn |
| `joint_marks` | true / false |  |  | required | the elbows, wrists, hip joints, knees and ankles are marked (they cannot be read from a silhouette) |

## Page `base_body`

the body's proportions, in eye heights above the sole

| slot | type | unit | range | need | meaning |
|---|---|---|---|---|---|
| `family` | one of reference / stylised |  |  | required | which body and face laws give the ranges: reference = bodies measured in 3D (the proportions of a lifelike figure), stylised = drawn sheets with a large head and large eyes (a lower knee, a lower shoulder, taller eyes and a higher brow) |
| `hips_y` | number | eye_height | reference: 0.64 – 0.67; stylised: 0.64 – 0.67 (from body.hips_y) | required | height of the hips |
| `hip_drop` | number | eye_height | reference: 0.01 – 0.08; stylised: 0.01 – 0.08 (from body.hip_drop) | required | the hip joints below the hips |
| `hip_x` | number | eye_height | reference: 0.04 – 0.06; stylised: 0.04 – 0.06 (from body.hip_x) | required | the hip joint's distance from the centre line |
| `knee` | number | eye_height | reference: 0.31 – 0.4; stylised: 0.27 – 0.38 (from body.knee) | required | height of the knee joint |
| `shoulder_x` | number | eye_height | reference: 0.05 – 0.08; stylised: 0.05 – 0.08 (from body.shoulder_x) | required | the shoulder joint's distance from the centre line |
| `arm` | number | eye_height | reference: 0.24 – 0.36; stylised: 0.24 – 0.36 (from body.arm) | required | shoulder to wrist |
| `elbow_split` | number | fraction | reference: 0.49 – 0.63; stylised: 0.49 – 0.63 (from body.elbow_split) | required | the elbow's place along the arm, from the shoulder |
| `face_length` | number | eye_height | reference: 0.04 – 0.07; stylised: 0.06 – 0.08 (from body.face_length) | required | the chin below the eyes |
| `head_top` | number | eye_height | reference: 1.07 – 1.12; stylised: 1.07 – 1.12 (from body.head_top) | required | the top of the skull, without hair |
| `shoulder` | number | eye_height | stylised: 0.86 – 0.9 (from body.shoulder) | required when family = stylised | height of the shoulder joints (the reference family fixes it at 0.9) |
| `skin_material` | a name from materials.items.name |  |  | required | what the skin is made of |

## Page `face`

the face from the front and the side, in chin-to-eye distances, and its expressions

| slot | type | unit | range | need | meaning |
|---|---|---|---|---|---|
| `eye_width` | number | chin_eye | reference: 0.63 – 0.72; stylised: 0.63 – 0.72 (from face.eye_width) | required | width of one eye |
| `eye_height` | number | chin_eye | reference: 0.24 – 0.33; stylised: 0.45 – 0.59 (from face.eye_height) | required | height of one eye |
| `eye_recess` | number | chin_eye | reference: 0.48 – 0.68; stylised: 0.48 – 0.68 (from face.eye_recess) | required | how far the eyes sit behind the front of the face |
| `brow_y` | number | chin_eye | reference: 1.23 – 1.32; stylised: 1.3 – 1.5 (from face.brow_y) | required | height of the brows |
| `brow_inner` | number | chin_eye | reference: 0.15 – 0.22; stylised: 0.15 – 0.22 (from face.brow_inner) | required | the inner end of a brow from the centre line |
| `brow_outer` | number | chin_eye | reference: 0.54 – 0.76; stylised: 0.54 – 0.76 (from face.brow_outer) | required | the outer end of a brow from the centre line |
| `mouth_y` | number | chin_eye | reference: 0.31 – 0.34; stylised: 0.26 – 0.47 (from face.mouth_y) | required | height of the mouth |
| `mouth_width` | number | chin_eye | reference: 0.41 – 0.54; stylised: 0.41 – 0.54 (from face.mouth_width) | required | width of the mouth |
| `depth` | number | chin_eye | reference: 1.69 – 1.84; stylised: 1.69 – 1.84 (from face.depth) | required | depth of the head, front to back |
| `eye_colour` | a name from palette.colours.name |  |  | required | the iris colour |
| `iris_material` | a name from materials.items.name |  |  | required | what the irises are made of (how they shine) |

Group `expressions` (1 to 16 items): expressions drawn on the face page

| field | type | unit | range | need | meaning |
|---|---|---|---|---|---|
| `name` | one of neutral / smile / laugh / surprise / anger / sad / shy / wink / closed |  |  | required | one expression |
| `note` | text (≤ 80 characters) |  |  | optional | what is particular about it |

## Page `hair`

the hair's style, where it is tied, its length and its strands

| slot | type | unit | range | need | meaning |
|---|---|---|---|---|---|
| `style_class` | one of short / bob / long_straight / long_wavy / ponytail / twin_tails / bun / braid |  |  | required | the kind of hair style |
| `length` | number | eye_height | 0.05 – 1.2 | required | the longest strands' length below the top of the skull |
| `parting` | number | deg | -40 – 40 | required | the parting's angle from the centre (0 = centred) |
| `fringe` | one of none / short / eyebrow / eye_covering |  |  | required | how far the fringe falls (eye_covering hides the brows: the face page then marks them) |
| `curl_amplitude` | number | mm | 0 – 15 | required | 0 for straight hair |
| `curl_pitch` | number | mm | 20 – 200 | required when curl_amplitude > 0 | length of one turn of a curl |
| `colour` | a name from palette.colours.name |  |  | required | the hair's colour |
| `material` | a name from materials.items.name |  |  | required | what the hair is made of |

Group `ties` (0 to 4 items): where bundles are gathered (ponytail, twin tails, bun)

| field | type | unit | range | need | meaning |
|---|---|---|---|---|---|
| `height` | number | fraction | 0 – 1 | required | height on the head, from the nape (0) to the crown (1) |
| `azimuth` | number | deg | -180 – 180 | required | around the head, 0 = the back, positive = the figure's left |
| `length` | number | eye_height | 0.02 – 1.2 | required | length of the gathered bundle |
| `accessory` | a name from accessories.items.name |  |  | optional | what ties it |

## Page `clothing`

each clothing layer, from the skin outwards


Group `layers` (0 to 8 items): one layer of clothing

| field | type | unit | range | need | meaning |
|---|---|---|---|---|---|
| `name` | text (≤ 40 characters) |  |  | required | a name for the layer (unique) |
| `order` | integer |  | 1 – 8 | required | the layer's place from the skin outwards (1 = on the skin) |
| `class` | one of skin / loose / chain / rigid / detached_sleeve |  |  | required | skin = follows the body, loose = drapes, chain = a hem or ribbon that swings, rigid = a hard part, detached_sleeve = a sleeve worn apart from any body garment |
| `part_of` | a name from clothing.layers.name |  |  | optional | the garment this layer belongs to (a sleeve of a jacket), when it is drawn as its own layer |
| `sleeves` | one of none / cap / short / elbow / long |  |  | optional | the sleeves of a garment that covers the shoulders |
| `from` | one of head_top / chin / shoulder / elbow / wrist / hips / hip_joint / knee / ankle / sole |  |  | required | the landmark the layer starts at |
| `to` | one of head_top / chin / shoulder / elbow / wrist / hips / hip_joint / knee / ankle / sole |  |  | required | the landmark the layer ends at |
| `hem` | number | fraction | 0 – 1 | required | where the hem falls between from (0) and to (1) |
| `ease` | number | mm | 0 – 200 | required | the gap between the layer and what is under it |
| `closure` | one of none / buttons / zip / laces / hooks / wrap |  |  | required | how it is closed |
| `material` | a name from materials.items.name |  |  | required | what it is made of |

## Page `accessories`

accessories and footwear, at their absolute size, attached to the body


Group `items` (0 to 16 items): one accessory or one piece of footwear

| field | type | unit | range | need | meaning |
|---|---|---|---|---|---|
| `name` | text (≤ 40 characters) |  |  | required | a name for the item (unique) |
| `kind` | one of footwear / ribbon / hair_ornament / jewellery / bag / hat / gloves / fastener / other |  |  | required | what it is (fastener = buttons, a zip or hooks on a garment) |
| `attach` | one of head_top / chin / shoulder / elbow / wrist / hips / hip_joint / knee / ankle / sole / hair_tie / closure_line |  |  | required | the landmark it is attached at (closure_line = along a garment's opening) |
| `on_layer` | a name from clothing.layers.name |  |  | optional | the garment it sits on, for a part of a garment |
| `size` | number | mm | 1 – 1000 | required | its largest dimension |
| `sole` | number | mm | 0 – 100 | required when kind = footwear | thickness of the sole under the ball of the foot |
| `heel` | number | mm | 0 – 150 | required when kind = footwear | how far the heel lifts the heel of the foot |
| `material` | a name from materials.items.name |  |  | required | what it is made of |

## Page `anatomy`

parts of the body that are not human anatomy (ears, tails, horns, wings), in the ear / tail part law's terms


Group `extras` (0 to 6 items): one extra part

| field | type | unit | range | need | meaning |
|---|---|---|---|---|---|
| `part` | one of ear / tail / horn / wing |  |  | required | the kind of part |
| `attach` | one of head_top / hips / shoulder |  |  | required | where it grows from |
| `pair` | true / false |  |  | required | one on each side (true) or one in the middle (false) |
| `length` | number | eye_height | 0.02 – 1.5 | required | length from root to tip |
| `material` | a name from materials.items.name |  |  | required | what it is made of (skin, fur as hair, …) |

## Page `materials`

what every part is made of and how light passes through it


Group `items` (1 to 24 items): one material

| field | type | unit | range | need | meaning |
|---|---|---|---|---|---|
| `name` | text (≤ 40 characters) |  |  | required | a name for the material (unique) |
| `class` | one of fabric / leather / metal / skin / hair / plastic / paper / glass / other |  |  | required | the kind of material |
| `colour` | a name from palette.colours.name |  |  | required | its base colour |
| `roughness` | number | fraction | 0 – 1 | required | 0 = mirror-smooth, 1 = fully rough |
| `metallic` | number | fraction | 0 – 1 | required | 0 = not metal, 1 = metal |
| `elasticity` | number | fraction | 0 – 1 | required | how far it stretches under a light pull (0 = not at all, 1 = doubles); a layer that follows the skin needs a stretchy material |
| `shade_shift` | number | fraction | -1 – 1 | required | where the shadow edge falls (toon shading) |
| `shade_softness` | number | fraction | 0 – 1 | required | 0 = soft shadow edge, 1 = hard |
| `outline` | number | mm | 0 – 5 | required | outline width (0 = none) |
| `transmission` | one of opaque / open_weave / diffuse / clear |  |  | required | how light passes through it, judged by eye: opaque = nothing behind shows; open_weave = skin or what is behind shows through the gaps of the threads (sheer cloth, lace, chiffon, organza, tulle, mesh); diffuse = light glows through but shapes behind blur away (paper, a shoji screen, frosted plastic, thick gauze); clear = what is behind is seen sharp (glass, clear plastic) |
| `openness` | number | fraction | 0 – 1 | required when transmission = open_weave | share of light passing straight through the weave |
| `thread_colour` | a name from palette.colours.name |  |  | required when transmission = open_weave | colour of the threads |
| `thickness` | number | mm | 0.01 – 10 | required when transmission = open_weave | thickness of the cloth |
| `stretch_sensitivity` | number | fraction | 0 – 1 | optional when transmission = open_weave | how much the weave opens when stretched |
| `transmittance` | number | fraction | 0 – 1 | required when transmission = diffuse | share of light passing through, scattered |
| `scattering` | number | fraction | 0 – 1 | required when transmission = diffuse | how much what is behind blurs (1 = only a glow) |
| `ior` | number | ratio | 1 – 3 | required when transmission = clear | index of refraction |
| `tint` | a name from palette.colours.name |  |  | optional when transmission = clear | the colour it tints what is behind |
| `pattern` | one of none / procedural / texture |  |  | required | how its surface varies |
| `pattern_law` | text (≤ 80 characters) |  |  | required when pattern = procedural | the material law that draws the pattern |
| `texture` | file (asset object) |  |  | required when pattern = texture | the image file of the pattern or print |
| `used_by` | text (≤ 200 characters) |  |  | required | which parts use it (layer and accessory names, skin, hair, eyes) |

## Page `palette`

the named colours every page refers to


Group `colours` (1 to 32 items): one named colour

| field | type | unit | range | need | meaning |
|---|---|---|---|---|---|
| `name` | text (≤ 40 characters) |  |  | required | a name for the colour (unique) |
| `srgb` | colour #rrggbb (sRGB) |  |  | required | the colour as #rrggbb in sRGB |

## What may be written on a page

- allowed `law_values`: every number, name and unit drawn on a page is a filled slot's value or a value of the laws the sheet uses (with its unit)
- allowed `page_titles`: the page names of this law and the slots' names as labels
- forbidden `tool_names`: no tool, product, engine, file format or law name drawn on a page (the sheet shows the character, not how it is made)
- forbidden `unsourced_claims`: no technical claim that no slot or law states (file size, precision, polygon or point counts, data formats, model kinds)
- forbidden `other_proportion_units`: no proportion in units the laws do not use (heads tall, a height of 1) and no proportion number that is not a slot's value

## Rules

- `clothing.layers.name` is unique: layer names differ
- `clothing.layers.order` is unique: no two layers in one place
- `accessories.items.name` is unique: item names differ
- `materials.items.name` is unique: material names differ
- `palette.colours.name` is unique: colour names differ
- `face.expressions.name` is unique: each expression is drawn once

## Chains

Landmarks from top to bottom; a span (`from` → `to`) lies on one chain, `from` above `to`.

- `body`: head_top → chin → shoulder → hips → hip_joint → knee → ankle → sole
- `arm`: shoulder → elbow → wrist

## Plausibility

An error stops the drawing; a warning is listed and the drawing goes on (check it is meant). `a->b` is field `b` of the item that field `a` names; `~` means the text mentions one of the words.

- error `along clothing.layers.from clothing.layers.to`: a layer covers a span of one chain (the body or an arm), from above to below
- error `when clothing.layers to=elbow|wrist require class=detached_sleeve or part_of`: a layer ending at the elbow or the wrist is a sleeve: it belongs to a garment (part_of) or is worn apart (detached_sleeve)
- error `when clothing.layers class=skin require ease<=3`: a layer that follows the skin has at most 3 mm of ease
- error `when clothing.layers class=skin require material->elasticity>=0.2`: a layer that follows the skin is made of a stretchy material
- error `when clothing.layers class=loose require ease>=5`: a loose layer has at least 5 mm of ease
- error `nesting clothing.layers order order ease ease along from to`: where two layers overlap, the outer one is not tighter than the inner one
- error `when accessories.items kind=fastener require attach=closure_line`: a fastener sits along a garment's opening
- error `when accessories.items kind=fastener require on_layer`: a fastener names the garment it closes
- error `when accessories.items kind=fastener require on_layer->closure!=none`: the garment a fastener sits on has a closure
- error `when accessories.items kind=footwear require attach=sole`: footwear is worn on the sole
- error `when accessories.items kind=gloves require attach=wrist`: gloves are attached at the wrist
- error `when accessories.items kind=hat require attach=head_top`: a hat sits on the top of the head
- error `when accessories.items kind=hair_ornament require attach=hair_tie|head_top`: a hair ornament sits on a tie or on the head
- error `when materials.items transmission=open_weave require class=fabric`: only cloth has an open weave
- error `when materials.items transmission=clear require class=glass|plastic`: only glass or plastic is clear
- error `when materials.items transmission=diffuse require class=paper|fabric|plastic`: light is scattered through paper, cloth or plastic
- warning `warn when clothing.layers name~sheer|lace|chiffon|organza|tulle|voile|mesh|gauze|see-through|see_through require material->transmission!=opaque`: a layer named as a see-through cloth is made of a material light passes through
- warning `warn when materials.items name~sheer|lace|chiffon|organza|tulle|voile|mesh|gauze|see-through|see_through require transmission!=opaque`: a material named as a see-through cloth lets light through
- warning `warn used palette.colours.name`: every colour of the palette is used somewhere
- warning `warn used materials.items.name`: every material is used by some part
- warning `warn when accessories.items kind=footwear require sole+heel<=turnaround.stature*80`: a style convention, not a measured bound: a sole and heel together above about 8 % of the stature lift the body further than the body law's upright stance describes
