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

// Twinkling star (param.y = 0) or shooting star streak (param.y = 1).
// param.x = phase (0..1), param.z = half width in uv units for streaks,
// param.w = twinkle rate (rad/s).
float4 PSMain(VSOut i) : SV_Target {
    float a;
    float3 rgb = i.color.rgb;
    if (i.param.y < 0.5) {
        float d = length(i.uv);
        float tw = 0.55 + 0.45 * sin(time * i.param.w + i.param.x * 6.2831853);
        float core = exp(-d * d * 28.0);
        float cross = exp(-abs(i.uv.x) * 22.0) * exp(-abs(i.uv.y) * 2.5)
                    + exp(-abs(i.uv.y) * 22.0) * exp(-abs(i.uv.x) * 2.5);
        a = saturate(core + cross * 0.45) * tw * (1.0 - smoothstep(0.75, 1.0, d)) * i.color.a;
    } else {
        // local +Y is the head, tail fades toward -Y
        float t = saturate((i.uv.y + 1.0) / 1.9);
        float w = max(i.param.z, 0.002) * (0.15 + 0.85 * t);
        float ax = abs(i.uv.x) / w;
        float across = exp(-ax * ax * 1.5);
        float tail = pow(t, 2.6);
        float2 hd = float2(i.uv.x * 4.0, (i.uv.y - 0.9) * 4.0);
        float head = exp(-dot(hd, hd) * 2.0);
        float edge = 1.0 - smoothstep(0.93, 1.0, abs(i.uv.y));
        a = saturate(across * tail + head) * edge * (1.0 - smoothstep(0.9, 1.0, abs(i.uv.x))) * i.color.a;
        rgb = lerp(rgb, float3(1.0, 1.0, 1.0), saturate(head));
    }
    return float4(rgb * a, a);
}
