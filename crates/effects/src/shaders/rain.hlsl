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

// param.w selects the shape: 0 = streak, 1 = droplet dot, 2 = ripple ring (flattened ellipse).
// Streak: local +Y is the direction of travel (head); param.x = half width in uv units.
float4 PSMain(VSOut i) : SV_Target {
    if (i.param.w > 1.5) {
        float2 q = float2(i.uv.x, i.uv.y * 3.0);
        float d = abs(length(q) - 0.8);
        float a = (1.0 - smoothstep(0.04, 0.16, d)) * i.color.a;
        return float4(i.color.rgb * a, a);
    }
    if (i.param.w > 0.5) {
        float r = length(i.uv);
        float a = (1.0 - smoothstep(0.5, 1.0, r)) * i.color.a;
        return float4(i.color.rgb * a, a);
    }
    float t = saturate(i.uv.y * 0.5 + 0.5);
    float w = max(i.param.x, 0.001);
    float ax = abs(i.uv.x) / w;
    float across = exp(-ax * ax * 1.6) * (1.0 - smoothstep(0.8, 1.6, ax));
    float along = pow(t, 1.6) * (1.0 - smoothstep(0.92, 1.0, abs(i.uv.y)));
    float a = saturate(across * along) * i.color.a;
    return float4(i.color.rgb * a, a);
}
