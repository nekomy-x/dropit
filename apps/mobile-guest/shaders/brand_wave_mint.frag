// DROP IT — фирменный знак «пиксельная волна» (brandbook, стр. 10).
//
// Ширина надписи разделена на 22 вертикальные полосы. У каждой полосы своя
// высота синей заливки; вместе они дают ступенчатый край волны ВНУТРИ букв
// (контур надписи неподвижен — его задаёт маска ShaderMask).
//
// Параметры из брендбука:
//   22 полосы · базовый уровень 0,49
//   амплитуды 0,20 и 0,12
//   временные множители 3,4 и −2,1
//   смягчение границы: ±0,025
//   палитра: светлая база ≈ #F2F7F5, синий #8AB4F8 → #AECBFA слева направо
//
// Знак статичен: волна живёт только внутри контура, размеры не меняются.

#include <flutter/runtime_effect.glsl>

uniform vec2 uSize;   // размер надписи в локальных пикселях
uniform float uTime;  // время в секундах

out vec4 fragColor;

const float BARS = 22.0;
const float BASE_LEVEL = 0.49;
const float AMP_1 = 0.20;
const float AMP_2 = 0.12;
const float TIME_1 = 3.4;
const float TIME_2 = -2.1;
const float SOFT = 0.025;

const vec3 LIGHT_BASE = vec3(0.949, 0.969, 0.961); // #F2F7F5
const vec3 MINT_DEEP = vec3(0.541, 0.706, 0.973);  // #8AB4F8
const vec3 MINT_SOFT = vec3(0.682, 0.796, 0.980);  // #AECBFA

void main() {
    vec2 uv = FlutterFragCoord().xy / uSize;
    uv = clamp(uv, 0.0, 1.0);

    // Ступенчатый сигнал: номер полосы и её фаза.
    float band = floor(uv.x * BARS);
    float phase = band / BARS;

    // Две синусоидальные волны задают уровень заливки полосы.
    float level = BASE_LEVEL
        + AMP_1 * sin(TIME_1 * uTime + phase * 6.2831853)
        + AMP_2 * sin(TIME_2 * uTime + phase * 10.8824770);
    level = clamp(level, 0.0, 1.0);

    // Заливка растёт снизу: граница полосы со смягчением ±0,025.
    float edge = 1.0 - level;
    float fill = smoothstep(edge - SOFT, edge + SOFT, uv.y);

    // Градиент слева направо: синий → светло-синий.
    vec3 accent = mix(MINT_DEEP, MINT_SOFT, uv.x);

    vec3 color = mix(LIGHT_BASE, accent, fill);
    fragColor = vec4(color, 1.0);
}
