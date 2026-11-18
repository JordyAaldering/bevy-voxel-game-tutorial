# Generating the world

So far we've only manually generated 3x3 chunks.
However, what we want is for the world to be infinitely generated.
Of course, we cannot generate the entire, infinite world all at once.
Instead, we should only generate chunks that are within some distance to the player.

Only generate chunks whose centre lies within a circular radius of the player's
actual world position. Use a loop over a square region to find candidate chunk
coordinates, then filter candidates by comparing the distance from the player's
`xz` position to the centre of each chunk. Converting the radius from chunks to
world units keeps it independent of the chunk dimensions.

## Disabling meshes

If the player has been exploring for a while, many chunks/meshes will still be active that the player can't even see.
Whenever a visible mesh is some distance away from the player, we disable that chunk.
The despawn radius should be slightly larger than the spawn radius. This
hysteresis prevents a chunk at the boundary from being repeatedly spawned and
despawned while the player moves back and forth. A padding of half a chunk is a
good starting point, stored in a separate constant so it can be tuned.

Just like the `NeedsRemesh` component, we add a `NeedsDespawn`.

I'll deliberately keep the radius very small (only the chunks directly around the player are generated and kept alive) so that it is clear to see what is going on.

## Tracking state of disabled meshes

Although we don't have any stateful voxels yet, like furnaces or crops, now is a good point to discuss how we'll deal with this in the future.
This is the kind of stuff you need to think about sooner rather than later, because it might influence your design. (In this case it won't)
I remember watching a [video](https://youtu.be/YIDbhVPHZbs) about how Terraria handles this.
When a chunk is disabled, they store the state of all the voxels in that chunk.
When the chunk is re-enabled, they adjust the timers for all voxels based on how long the chunk was disabled.
At least, that's what I assumed based on the video.
Turns out, the video was wrong (the title of the video has been renamed accordingly).
That just goes to show you shouldn't blindly trust the first bit of information you find.
Still, the approach is valid and elegant, and can work very well here.
This won't work for more complicated systems, like mob forms or redstone circuits.
