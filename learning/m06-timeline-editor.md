# Milestone 6 Learning Notes: Timeline Editor

1. Enforcing the 500ms minimum scene duration inside a PostgreSQL `FOR UPDATE` transaction guarantees local boundary invariance on both single-cut and batch timeline edits.
2. Drawing waveforms on a device-pixel-ratio `<canvas>` instead of hundreds of DOM bars keeps Track A1 crisp in light and dark modes while scaling smoothly to 40-minute timelines.
3. Tracking Alt-key snap inversion, boundary undo/redo snapshots, and flagged-cut triage directly in Step 4 turns alignment warnings into fast, one-click manual corrections.
