# Vector

You're working in the Vector space. The object on the stage is a drawing: an SVG document in TSX (its edit.tsx),
1280×720, whose shapes are a fold over what the panels publish and the pointer on the stage.

- The tools row picks the tool (Pen, Rect, Ellipse) and the colour; the person then draws on the stage. Pen adds a
  point per press; a press on the first point closes the shape, Enter ends it open, Escape drops it.
- You can draw too, through the layers panel's actions: `layers.add` with a rect, ellipse or path in the drawing's
  pixels, `layers.remove` by row, `layers.clear`. Rows count from the top (the topmost shape first).
- To make something lasting or larger than a few shapes, write it into the object's edit.tsx as SVG (its START list),
  as you would any React component.
