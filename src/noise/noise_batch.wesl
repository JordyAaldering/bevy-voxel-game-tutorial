@group(0) @binding(0) var<storage, read> params: array<u32>;
@group(0) @binding(1) var<storage, read_write> results: array<f32>;

@compute @workgroup_size(64)
fn noise_batch(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    let width = params[2];
    let height = params[3];
    if i >= width * width * height { return; }
    let x = i % width;
    let y = (i / width) % height;
    let z = i / (width * height);
    let wx = bitcast<i32>(params[0]) + i32(x);
    let wz = bitcast<i32>(params[1]) + i32(z);
    let position = vec3<f32>(f32(wx) + bitcast<f32>(params[5]), f32(y), f32(wz) + bitcast<f32>(params[6]));
    let seed = params[4];
    var frequency = bitcast<f32>(params[8]);
    var amplitude = 1.0;
    var total = 0.0;
    var weight = 0.0;
    for (var octave = 0u; octave < params[7]; octave++) {
        let p = position * frequency;
        var sample: f32;
        if params[10] == 0u {
            sample = voronoi_noise3(bitcast<i32>(seed + octave), p);
        } else {
            sample = simplex_noise3(bitcast<i32>(seed + octave), p);
        }
        total += amplitude * sample;
        weight += amplitude;
        amplitude *= bitcast<f32>(params[9]);
        frequency *= bitcast<f32>(params[1035]);
    }
    results[i] = select(bitcast<f32>(params[1037]), (total / weight) * bitcast<f32>(params[1036]) + bitcast<f32>(params[1037]), weight != 0.0);
}
