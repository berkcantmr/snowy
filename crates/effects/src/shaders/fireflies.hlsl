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

// Glowing firefly. param.x = phase (0..1), param.y = blink rate (rad/s).
float4 PSMain(VSOut i) : SV_Target {
    float d = length(i.uv);
    float blink = 0.5 + 0.5 * sin(time * i.param.y + i.param.x * 6.2831853);
    blink = smoothstep(0.05, 0.95, blink);
    float core = exp(-d * d * 55.0);
    float halo = exp(-d * d * 7.0) * 0.4;
    float fade = 1.0 - smoothstep(0.7, 1.0, d);
    float glow = (core + halo) * fade * (0.12 + 0.88 * blink);
    float a = saturate(glow) * i.color.a;
    float3 rgb = lerp(i.color.rgb, float3(1.0, 1.0, 0.7), saturate(core * blink));
    return float4(rgb * a, a);
}
