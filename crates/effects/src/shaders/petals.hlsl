// Snowy effect shader. Follows SHADER_CONTRACT (snowy-core).
cbuffer Frame : register(b0) { float2 viewport; float time; float _pad; };

struct VSIn {
    float2 pos   : POS;
    float  size  : SIZE;
    float  rot   : ROT;
    float4 color : COLOR;
    float4 param : PARAM;
    uint   vid   : SV_VertexID;
};

struct VSOut {
    float4 sv    : SV_Position;
    float2 uv    : TEXCOORD0;
    float4 color : COLOR;
    float4 param : TEXCOORD1;
};

VSOut VSMain(VSIn v) {
    float2 corner = float2((v.vid & 1) ? 1.0 : -1.0, (v.vid >> 1) ? 1.0 : -1.0);
    float s, c;
    sincos(v.rot, s, c);
    float2 local = corner * v.size;
    float2 rotated = float2(c * local.x - s * local.y, s * local.x + c * local.y);
    float2 px = v.pos + rotated;
    VSOut o;
    o.sv = float4(px.x / viewport.x * 2.0 - 1.0, 1.0 - px.y / viewport.y * 2.0, 0.0, 1.0);
    o.uv = corner;
    o.color = v.color;
    o.param = v.param;
    return o;
}

// Petal / leaf. param.x = flip phase (radians), param.y = shape (0 petal, 1 leaf),
// param.z = per-instance seed 0..1.
float4 PSMain(VSOut i) : SV_Target {
    float flip = cos(i.param.x);
    float c = max(abs(flip), 0.12);
    float2 p = float2(i.uv.x / c, i.uv.y);
    float shade = 0.7 + 0.3 * abs(flip);
    float seed = i.param.z;
    float alpha;
    float3 rgb = i.color.rgb;
    if (i.param.y < 0.5) {
        // Cherry petal: ellipse with a notch at the -Y end.
        float2 q = float2(p.x * 1.3, p.y * 0.92 + 0.04);
        float d = length(q) - 1.0;
        alpha = 1.0 - smoothstep(-0.06, 0.04, d);
        float n = (-p.y - 0.5) - abs(p.x) * 1.9;
        alpha *= 1.0 - smoothstep(-0.05, 0.06, n);
        float tip = saturate(p.y * 0.5 + 0.5);
        rgb = lerp(rgb * 1.08 + 0.05, rgb * 0.82, tip * 0.8);
        rgb = lerp(rgb, float3(1.0, 0.97, 0.98), 0.25 * (1.0 - tip) * (1.0 - abs(p.x)));
    } else {
        // Leaf: pointed ellipse with a centre vein and side veins.
        float y = p.y;
        float hw = 0.58 * pow(saturate(1.0 - y * y), 0.75);
        float d = abs(p.x) - hw;
        alpha = (1.0 - smoothstep(-0.05, 0.05, d)) * (1.0 - smoothstep(0.93, 1.0, abs(y)));
        float vein = 1.0 - smoothstep(0.02, 0.07, abs(p.x));
        float side = 1.0 - smoothstep(0.0, 0.05, abs(frac((abs(p.x) * 2.0 + y * 1.5 + seed) * 3.0) - 0.5) * 0.5 - 0.02);
        side *= step(0.08, abs(p.x)) * (1.0 - smoothstep(0.7, 1.0, abs(p.x) / max(hw, 0.01))) * 0.5;
        rgb = lerp(rgb, rgb * 0.55, saturate(vein * 0.8 + side * 0.35));
        rgb *= 0.9 + 0.2 * seed;
    }
    alpha *= i.color.a * saturate(1.0 - smoothstep(0.98, 1.0, abs(i.uv.x)));
    rgb = saturate(rgb * shade);
    return float4(rgb * alpha, alpha);
}
