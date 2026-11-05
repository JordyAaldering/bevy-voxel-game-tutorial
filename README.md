
> Give a man a voxel, and you feed him for a day. Teach a man to voxel, and you feed him for a lifetime.

# Building a Voxel Game with Rust and Bevy

![](docs/img/chapter17.png)

Learn how to build a voxel game using Rust and the Bevy game engine.

This guide explores the fundamentals of voxel-engine development, from representing blocks in a 3D world to generating meshes and building a chunk-based terrain system.
Each chapter explains the underlying concepts and design decisions, helping you understand how the systems work rather than simply copying code.

There's no shortage of voxel tutorials, so why write another one?
Because I don't want this to be just a recipe to follow.
It's easy to copy someone else's code and get the same result, but that doesn't necessarily teach you how to build or adapt it yourself.
If you've tried making a voxel game before, you might have come across the tutorials by _b3agz_.
I mention him because he describes very well the problem of many tutorials ([_This Channel is Dead_](https://youtu.be/g0V1JM9P4z8)).
There's a difference between _showing_ someone how to do something and _teaching_ them how to do it.
I want these guides to do the latter: explain the ideas behind the code, help you understand the choices we're making, and give you room to work things out for yourself.
The goal is for you to leave with the skills to keep building, not with a project that only works as long as I keep writing and you keep following along.
I deliberately won't provide complete code snippets.
That means more effort on your part, but it should ultimately help you understand your code.

We won't focus solely on making a perfect voxel engine from start to finish.
It gives us room to explore more than just generating a world of blocks.
Along the way, we'll build a telemetry web server, learn how simplex noise works, create an editor, and more.
We could use libraries, of course, but there's a lot of satisfaction in understanding something well enough to build it yourself.
If all you want is a voxel world, you could use a library like [bevy-voxel-world](https://crates.io/crates/bevy_voxel_world).

I'm no Bevy or game-design professional, so don't expect everything here to be optimal, if even correct.
Bevy is a fast-moving target, so code might be out of date, and some solutions might not follow established Bevy conventions.
Treat this guide as a starting point, and feel free to suggest improvements in a GitHub issue.
If you find the guide useful, please star the repository, so I know to keep working on it.

Before starting, I recommend reading the [Bevy introduction](https://bevy.org/news/introducing-bevy).
It's a little outdated, but it explains the core ideas well.
In particular, the _Entity Component System_ (ECS) is an important concept, as it lies at the core of everything Bevy does.

## Chapters

1.  [Setting the scene](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Setting-the-scene)
2.  [Inspecting settings](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Inspecting-settings)
3.  [The first mesh](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/The-first-mesh)
4.  [Creating a chunk](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Creating-a-chunk)
5.  [Multiple chunks](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Multiple-chunks)
6.  [Multiple voxel types](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Multiple-voxel-types)
7.  [Creating a player character](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Creating-a-player-character)
8.  [Breaking and placing blocks](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Breaking-and-placing-blocks)
9.  [Terrain Generation](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Terrain-Generation)
10. [Generating the world](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Generating-the-world)
11. [Performance debugging](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Performance-debugging)
12. [Editor and refactoring](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Editor-and-refactoring)
13. [Phase'd chunk generation](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Phase%27d-chunk-generation)
14. [Serialization](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Serialization)
15. [Scaling voxel types](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Scaling-voxel-types)
16. [Trees, finally!](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Trees%2C-finally%21)
17. [Pump up the noise](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Pump-up-the-noise)
18. [Shader programming](https://github.com/JordyAaldering/bevy-voxel-game-tutorial/wiki/Shader-programming)
