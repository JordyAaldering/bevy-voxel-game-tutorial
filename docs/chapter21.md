# Structure editor

Trees look a bit weird in our marching cube terrain, because they are made of cubes.
Later, we'll replace them with a mesh, but that would mean we no longer have any structures.
So, before replacing them, it would be nice to have some other structures in the world, like houses.
Already for trees, editing the JSON directly was cumbersome, image how bad it would be for more complicated structures.
So, let's make a _structure editor_.

You could implement different types of editor.
Perhaps you want an editor that behaves like 3D modelling software, where you can move around the camera and place blocks.
Or perhaps it would be better for your workflow if the editor is more like a 2D tilemap editor, where you can only edit one layer at a time.
For me, the most natural approach would be to have the editor behave similar to the game itself, where you can move around and place blocks in 'creative mode'.

For this, we need to add a new binary to our project, which will be the structure editor.
This editor needs a creative mode player, which can fly around and place blocks.
It needs some way to save and load a structure from a file.
Because the origin is important for determining where the structure is placed in the world, we also need to show this point.
