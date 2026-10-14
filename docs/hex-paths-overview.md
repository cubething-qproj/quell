## From landscape to trails

The hex dev screen separates four stages:

| Type | Owns | Reused when |
| --- | --- | --- |
| `Region` | A large eroded height raster, drainage receivers, and upstream areas | Region settings and seed match the cache |
| `Terrain` | A chosen hex board, original corner heights, creeks, and site scores | A layout regenerates |
| `Layout` | Landmarks, levelled pads, trails, crossings, and tease pairs | The player inspects a different view |
| `Board` | Terrain, the current layout, and measurements for the preview | The preview updates its meshes and markers |

Generation operates on Rust data without spawning entities. Bevy systems
install the resulting `Board` and render it. This makes the same layout
algorithm usable for first placement and pinned regeneration.

## Landscape and water

Regional erosion repeats uplift, priority flooding, downhill routing, channel
cutting, and slope creep. Flooding raises depressions enough to give every
cell a descending route to a boundary outlet. Flow accumulation measures the
area feeding each cell, which determines where channels can form.

Window selection ranks crops for creeks, low-bank ford sites, natural views,
relief, and the absence of river-sized flow. It samples the selected ground
onto hex corners. Regional heights are in metres; terrain and layout heights
are in hex edge lengths.

Trails and creeks use different graphs. Trails follow the honeycomb's corners
and sides; creeks join tile centres. A creek link crosses the shared side of
its two tiles, closing that side to trails unless a crossing pad opens it.

## Landmarks and routes

Placement chooses dry, reachable sites for POIs and benches and main-creek
sites for crossings. Each layout copies the original heights, then levels its
pads. Regeneration starts from those original heights again, so a moved pad
does not leave a permanent flattened scar.

The trail builder compares terrain routes with routes along trails already
laid. A greedy spanner adds useful connections; later passes repair
connectivity, attempt loops, connect crossing banks, and join the open ends
of pinned trails. The loop test contracts pads before finding biconnected
blocks, so walking around a pad cannot count as an independent way onward.

A tease pairs a viewer with a POI that should remain a substantial walk away.
An initial layout establishes arrival order. A second seeded build chooses
pairs and guards new routes against shortening their detours or reversing
that order. Required connections can disqualify a pair and cause a retry.
Visibility is checked separately when reporting how many teases hold.

## Regeneration and presentation

Pins describe the player tile, nodes and trail edges to retain, and nodes
that may move. Partial regeneration keeps nearby content; Fear preserves
less. Crossings stay fixed, and regeneration is refused while standing on one.
Node IDs and cards survive regeneration. A fresh New layout operation deals
again.

The preview rebuilds its surface and markers through events after board or
appearance changes. View changes update colours and reveal materials. Marker
pointer observers select camera focus. Vertical exaggeration affects drawing
and camera targets, not routing, sight calculations, or pace estimates.

Generation is synchronous, with the last region cached. Walking, woodland
sight, and smoothed trail rendering are not implemented here. The reported
walk times average both directions and assume shorter smoothed trails; they
are estimates rather than measurements of a traversed route.

For screen lifecycle details, start at
[`HexPathsScreen`](crate::prelude::HexPathsScreen). The module comments under
`src/screen/dev/hex_paths/` describe the terrain, layout, and preview APIs.
