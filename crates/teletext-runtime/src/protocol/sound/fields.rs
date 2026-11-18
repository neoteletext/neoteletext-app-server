//! The named fields and named parameters of every family's patch
//! (server.md SRV-26 to SRV-45), as the family documents name and
//! bound them. Generated once from server.md's tables and kept as source;
//! the server holds no packing of them (SRV-53, SRV-55): each family's
//! call site in `sound/family/` hands them to the engine.

use schemars::JsonSchema;
use serde::Deserialize;

use super::ranged::{U8In, U16In};
use super::{Synth7Name, Synth60Chorus, Synth60Pwm, Synth80Tone, Synth106Chorus, Synth106Polarity, Synth106Pwm, Synth106Range, Synth106Vca};

/// One operator of a synth7 voice: its 21 voice parameters in the order of
/// their offsets within the operator (synth7.md, S7-28; server.md SRV-26).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth7Operator {
    /// EG RATE 1 to 4, each 0 to 99 (offsets 0 to 3).
    pub eg_rates: [U8In<0, 99>; 4],
    /// EG LEVEL 1 to 4, each 0 to 99 (offsets 4 to 7).
    pub eg_levels: [U8In<0, 99>; 4],
    /// KEYBOARD LEVEL SCALE BREAK POINT, 0 to 99 (offset 8).
    pub break_point: U8In<0, 99>,
    /// KEYBOARD LEVEL SCALE LEFT DEPTH, 0 to 99 (offset 9).
    pub left_depth: U8In<0, 99>,
    /// KEYBOARD LEVEL SCALE RIGHT DEPTH, 0 to 99 (offset 10).
    pub right_depth: U8In<0, 99>,
    /// KEYBOARD LEVEL SCALE LEFT CURVE: 0 -LIN, 1 -EXP, 2 +EXP, 3 +LIN (offset 11).
    pub left_curve: U8In<0, 3>,
    /// KEYBOARD LEVEL SCALE RIGHT CURVE, as the left (offset 12).
    pub right_curve: U8In<0, 3>,
    /// KEYBOARD RATE SCALING, 0 to 7 (offset 13).
    pub rate_scaling: U8In<0, 7>,
    /// MOD SENSITIVITY AMPLITUDE, 0 to 3 (offset 14).
    pub amplitude_sensitivity: U8In<0, 3>,
    /// OPERATOR KEY VELOCITY SENSITIVITY, 0 to 7 (offset 15).
    pub velocity_sensitivity: U8In<0, 7>,
    /// OPERATOR OUTPUT LEVEL, 0 to 99 (offset 16).
    pub output_level: U8In<0, 99>,
    /// OSCILLATOR MODE: 0 ratio, 1 fixed (offset 17).
    pub oscillator_mode: U8In<0, 1>,
    /// OSCILLATOR FREQUENCY COARSE, 0 to 31 (offset 18).
    pub frequency_coarse: U8In<0, 31>,
    /// OSCILLATOR FREQUENCY FINE, 0 to 99 (offset 19).
    pub frequency_fine: U8In<0, 99>,
    /// DETUNE, 0 to 14, 7 none (offset 20).
    pub detune: U8In<0, 14>,
}

/// A synth7 voice as named fields: every voice parameter of synth7.md, S7-28,
/// in the interface's own values (server.md SRV-26). The operator switch
/// (parameter 155) is not a field, since no stored voice holds it (S7-30).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth7Voice {
    /// The six operators, operator 1 first: operator n's fields are voice parameters (6 - n) x 21 to (6 - n) x 21 + 20.
    pub operators: [Synth7Operator; 6],
    /// PITCH EG RATE 1 to 4, each 0 to 99 (parameters 126 to 129).
    pub pitch_eg_rates: [U8In<0, 99>; 4],
    /// PITCH EG LEVEL 1 to 4, each 0 to 99 (parameters 130 to 133).
    pub pitch_eg_levels: [U8In<0, 99>; 4],
    /// ALGORITHM SELECT, 0 to 31: 0 is the algorithm the instrument shows as 1 (parameter 134).
    pub algorithm: U8In<0, 31>,
    /// FEED BACK, 0 to 7 (parameter 135).
    pub feedback: U8In<0, 7>,
    /// OSCILLATOR SYNC, 0 or 1 (parameter 136).
    pub oscillator_sync: U8In<0, 1>,
    /// LFO SPEED, 0 to 99 (parameter 137).
    pub lfo_speed: U8In<0, 99>,
    /// LFO DELAY, 0 to 99 (parameter 138).
    pub lfo_delay: U8In<0, 99>,
    /// LFO PMD, 0 to 99 (parameter 139).
    pub lfo_pitch_depth: U8In<0, 99>,
    /// LFO AMD, 0 to 99 (parameter 140).
    pub lfo_amplitude_depth: U8In<0, 99>,
    /// LFO SYNC, 0 or 1 (parameter 141).
    pub lfo_sync: U8In<0, 1>,
    /// LFO WAVE: 0 triangle, 1 sawtooth down, 2 sawtooth up, 3 square, 4 sine, 5 sample and hold (parameter 142).
    pub lfo_wave: U8In<0, 5>,
    /// MOD SENSITIVITY PITCH, 0 to 7 (parameter 143).
    pub pitch_sensitivity: U8In<0, 7>,
    /// TRANSPOSE, 0 to 48, 24 none (parameter 144).
    pub transpose: U8In<0, 48>,
    /// VOICE NAME, 0 to 10 characters each U+0020 to U+007E, padded with spaces to ten (parameters 145 to 154).
    pub name: Synth7Name,
}

/// One of synth7's 170 named parameters and its value (server.md SRV-28):
/// each voice parameter 0 to 155 of S7-28 and function parameter 64 to 77
/// of S7-33, written as the interface's parameter change (S7-43).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(non_camel_case_types, reason = "each variant is the parameter's name on the wire, which the schema and the kits read as written (server.md SRV-22)")]
pub enum Synth7Parameter {
    /// Operator 1's EG RATE 1: voice parameter 105.
    op1_eg_rate_1(U8In<0, 99>),
    /// Operator 1's EG RATE 2: voice parameter 106.
    op1_eg_rate_2(U8In<0, 99>),
    /// Operator 1's EG RATE 3: voice parameter 107.
    op1_eg_rate_3(U8In<0, 99>),
    /// Operator 1's EG RATE 4: voice parameter 108.
    op1_eg_rate_4(U8In<0, 99>),
    /// Operator 1's EG LEVEL 1: voice parameter 109.
    op1_eg_level_1(U8In<0, 99>),
    /// Operator 1's EG LEVEL 2: voice parameter 110.
    op1_eg_level_2(U8In<0, 99>),
    /// Operator 1's EG LEVEL 3: voice parameter 111.
    op1_eg_level_3(U8In<0, 99>),
    /// Operator 1's EG LEVEL 4: voice parameter 112.
    op1_eg_level_4(U8In<0, 99>),
    /// Operator 1's KEYBOARD LEVEL SCALE BREAK POINT: voice parameter 113.
    op1_break_point(U8In<0, 99>),
    /// Operator 1's KEYBOARD LEVEL SCALE LEFT DEPTH: voice parameter 114.
    op1_left_depth(U8In<0, 99>),
    /// Operator 1's KEYBOARD LEVEL SCALE RIGHT DEPTH: voice parameter 115.
    op1_right_depth(U8In<0, 99>),
    /// Operator 1's KEYBOARD LEVEL SCALE LEFT CURVE: voice parameter 116.
    op1_left_curve(U8In<0, 3>),
    /// Operator 1's KEYBOARD LEVEL SCALE RIGHT CURVE: voice parameter 117.
    op1_right_curve(U8In<0, 3>),
    /// Operator 1's KEYBOARD RATE SCALING: voice parameter 118.
    op1_rate_scaling(U8In<0, 7>),
    /// Operator 1's MOD SENSITIVITY AMPLITUDE: voice parameter 119.
    op1_amplitude_sensitivity(U8In<0, 3>),
    /// Operator 1's OPERATOR KEY VELOCITY SENSITIVITY: voice parameter 120.
    op1_velocity_sensitivity(U8In<0, 7>),
    /// Operator 1's OPERATOR OUTPUT LEVEL: voice parameter 121.
    op1_output_level(U8In<0, 99>),
    /// Operator 1's OSCILLATOR MODE: voice parameter 122.
    op1_oscillator_mode(U8In<0, 1>),
    /// Operator 1's OSCILLATOR FREQUENCY COARSE: voice parameter 123.
    op1_frequency_coarse(U8In<0, 31>),
    /// Operator 1's OSCILLATOR FREQUENCY FINE: voice parameter 124.
    op1_frequency_fine(U8In<0, 99>),
    /// Operator 1's DETUNE: voice parameter 125.
    op1_detune(U8In<0, 14>),
    /// Operator 2's EG RATE 1: voice parameter 84.
    op2_eg_rate_1(U8In<0, 99>),
    /// Operator 2's EG RATE 2: voice parameter 85.
    op2_eg_rate_2(U8In<0, 99>),
    /// Operator 2's EG RATE 3: voice parameter 86.
    op2_eg_rate_3(U8In<0, 99>),
    /// Operator 2's EG RATE 4: voice parameter 87.
    op2_eg_rate_4(U8In<0, 99>),
    /// Operator 2's EG LEVEL 1: voice parameter 88.
    op2_eg_level_1(U8In<0, 99>),
    /// Operator 2's EG LEVEL 2: voice parameter 89.
    op2_eg_level_2(U8In<0, 99>),
    /// Operator 2's EG LEVEL 3: voice parameter 90.
    op2_eg_level_3(U8In<0, 99>),
    /// Operator 2's EG LEVEL 4: voice parameter 91.
    op2_eg_level_4(U8In<0, 99>),
    /// Operator 2's KEYBOARD LEVEL SCALE BREAK POINT: voice parameter 92.
    op2_break_point(U8In<0, 99>),
    /// Operator 2's KEYBOARD LEVEL SCALE LEFT DEPTH: voice parameter 93.
    op2_left_depth(U8In<0, 99>),
    /// Operator 2's KEYBOARD LEVEL SCALE RIGHT DEPTH: voice parameter 94.
    op2_right_depth(U8In<0, 99>),
    /// Operator 2's KEYBOARD LEVEL SCALE LEFT CURVE: voice parameter 95.
    op2_left_curve(U8In<0, 3>),
    /// Operator 2's KEYBOARD LEVEL SCALE RIGHT CURVE: voice parameter 96.
    op2_right_curve(U8In<0, 3>),
    /// Operator 2's KEYBOARD RATE SCALING: voice parameter 97.
    op2_rate_scaling(U8In<0, 7>),
    /// Operator 2's MOD SENSITIVITY AMPLITUDE: voice parameter 98.
    op2_amplitude_sensitivity(U8In<0, 3>),
    /// Operator 2's OPERATOR KEY VELOCITY SENSITIVITY: voice parameter 99.
    op2_velocity_sensitivity(U8In<0, 7>),
    /// Operator 2's OPERATOR OUTPUT LEVEL: voice parameter 100.
    op2_output_level(U8In<0, 99>),
    /// Operator 2's OSCILLATOR MODE: voice parameter 101.
    op2_oscillator_mode(U8In<0, 1>),
    /// Operator 2's OSCILLATOR FREQUENCY COARSE: voice parameter 102.
    op2_frequency_coarse(U8In<0, 31>),
    /// Operator 2's OSCILLATOR FREQUENCY FINE: voice parameter 103.
    op2_frequency_fine(U8In<0, 99>),
    /// Operator 2's DETUNE: voice parameter 104.
    op2_detune(U8In<0, 14>),
    /// Operator 3's EG RATE 1: voice parameter 63.
    op3_eg_rate_1(U8In<0, 99>),
    /// Operator 3's EG RATE 2: voice parameter 64.
    op3_eg_rate_2(U8In<0, 99>),
    /// Operator 3's EG RATE 3: voice parameter 65.
    op3_eg_rate_3(U8In<0, 99>),
    /// Operator 3's EG RATE 4: voice parameter 66.
    op3_eg_rate_4(U8In<0, 99>),
    /// Operator 3's EG LEVEL 1: voice parameter 67.
    op3_eg_level_1(U8In<0, 99>),
    /// Operator 3's EG LEVEL 2: voice parameter 68.
    op3_eg_level_2(U8In<0, 99>),
    /// Operator 3's EG LEVEL 3: voice parameter 69.
    op3_eg_level_3(U8In<0, 99>),
    /// Operator 3's EG LEVEL 4: voice parameter 70.
    op3_eg_level_4(U8In<0, 99>),
    /// Operator 3's KEYBOARD LEVEL SCALE BREAK POINT: voice parameter 71.
    op3_break_point(U8In<0, 99>),
    /// Operator 3's KEYBOARD LEVEL SCALE LEFT DEPTH: voice parameter 72.
    op3_left_depth(U8In<0, 99>),
    /// Operator 3's KEYBOARD LEVEL SCALE RIGHT DEPTH: voice parameter 73.
    op3_right_depth(U8In<0, 99>),
    /// Operator 3's KEYBOARD LEVEL SCALE LEFT CURVE: voice parameter 74.
    op3_left_curve(U8In<0, 3>),
    /// Operator 3's KEYBOARD LEVEL SCALE RIGHT CURVE: voice parameter 75.
    op3_right_curve(U8In<0, 3>),
    /// Operator 3's KEYBOARD RATE SCALING: voice parameter 76.
    op3_rate_scaling(U8In<0, 7>),
    /// Operator 3's MOD SENSITIVITY AMPLITUDE: voice parameter 77.
    op3_amplitude_sensitivity(U8In<0, 3>),
    /// Operator 3's OPERATOR KEY VELOCITY SENSITIVITY: voice parameter 78.
    op3_velocity_sensitivity(U8In<0, 7>),
    /// Operator 3's OPERATOR OUTPUT LEVEL: voice parameter 79.
    op3_output_level(U8In<0, 99>),
    /// Operator 3's OSCILLATOR MODE: voice parameter 80.
    op3_oscillator_mode(U8In<0, 1>),
    /// Operator 3's OSCILLATOR FREQUENCY COARSE: voice parameter 81.
    op3_frequency_coarse(U8In<0, 31>),
    /// Operator 3's OSCILLATOR FREQUENCY FINE: voice parameter 82.
    op3_frequency_fine(U8In<0, 99>),
    /// Operator 3's DETUNE: voice parameter 83.
    op3_detune(U8In<0, 14>),
    /// Operator 4's EG RATE 1: voice parameter 42.
    op4_eg_rate_1(U8In<0, 99>),
    /// Operator 4's EG RATE 2: voice parameter 43.
    op4_eg_rate_2(U8In<0, 99>),
    /// Operator 4's EG RATE 3: voice parameter 44.
    op4_eg_rate_3(U8In<0, 99>),
    /// Operator 4's EG RATE 4: voice parameter 45.
    op4_eg_rate_4(U8In<0, 99>),
    /// Operator 4's EG LEVEL 1: voice parameter 46.
    op4_eg_level_1(U8In<0, 99>),
    /// Operator 4's EG LEVEL 2: voice parameter 47.
    op4_eg_level_2(U8In<0, 99>),
    /// Operator 4's EG LEVEL 3: voice parameter 48.
    op4_eg_level_3(U8In<0, 99>),
    /// Operator 4's EG LEVEL 4: voice parameter 49.
    op4_eg_level_4(U8In<0, 99>),
    /// Operator 4's KEYBOARD LEVEL SCALE BREAK POINT: voice parameter 50.
    op4_break_point(U8In<0, 99>),
    /// Operator 4's KEYBOARD LEVEL SCALE LEFT DEPTH: voice parameter 51.
    op4_left_depth(U8In<0, 99>),
    /// Operator 4's KEYBOARD LEVEL SCALE RIGHT DEPTH: voice parameter 52.
    op4_right_depth(U8In<0, 99>),
    /// Operator 4's KEYBOARD LEVEL SCALE LEFT CURVE: voice parameter 53.
    op4_left_curve(U8In<0, 3>),
    /// Operator 4's KEYBOARD LEVEL SCALE RIGHT CURVE: voice parameter 54.
    op4_right_curve(U8In<0, 3>),
    /// Operator 4's KEYBOARD RATE SCALING: voice parameter 55.
    op4_rate_scaling(U8In<0, 7>),
    /// Operator 4's MOD SENSITIVITY AMPLITUDE: voice parameter 56.
    op4_amplitude_sensitivity(U8In<0, 3>),
    /// Operator 4's OPERATOR KEY VELOCITY SENSITIVITY: voice parameter 57.
    op4_velocity_sensitivity(U8In<0, 7>),
    /// Operator 4's OPERATOR OUTPUT LEVEL: voice parameter 58.
    op4_output_level(U8In<0, 99>),
    /// Operator 4's OSCILLATOR MODE: voice parameter 59.
    op4_oscillator_mode(U8In<0, 1>),
    /// Operator 4's OSCILLATOR FREQUENCY COARSE: voice parameter 60.
    op4_frequency_coarse(U8In<0, 31>),
    /// Operator 4's OSCILLATOR FREQUENCY FINE: voice parameter 61.
    op4_frequency_fine(U8In<0, 99>),
    /// Operator 4's DETUNE: voice parameter 62.
    op4_detune(U8In<0, 14>),
    /// Operator 5's EG RATE 1: voice parameter 21.
    op5_eg_rate_1(U8In<0, 99>),
    /// Operator 5's EG RATE 2: voice parameter 22.
    op5_eg_rate_2(U8In<0, 99>),
    /// Operator 5's EG RATE 3: voice parameter 23.
    op5_eg_rate_3(U8In<0, 99>),
    /// Operator 5's EG RATE 4: voice parameter 24.
    op5_eg_rate_4(U8In<0, 99>),
    /// Operator 5's EG LEVEL 1: voice parameter 25.
    op5_eg_level_1(U8In<0, 99>),
    /// Operator 5's EG LEVEL 2: voice parameter 26.
    op5_eg_level_2(U8In<0, 99>),
    /// Operator 5's EG LEVEL 3: voice parameter 27.
    op5_eg_level_3(U8In<0, 99>),
    /// Operator 5's EG LEVEL 4: voice parameter 28.
    op5_eg_level_4(U8In<0, 99>),
    /// Operator 5's KEYBOARD LEVEL SCALE BREAK POINT: voice parameter 29.
    op5_break_point(U8In<0, 99>),
    /// Operator 5's KEYBOARD LEVEL SCALE LEFT DEPTH: voice parameter 30.
    op5_left_depth(U8In<0, 99>),
    /// Operator 5's KEYBOARD LEVEL SCALE RIGHT DEPTH: voice parameter 31.
    op5_right_depth(U8In<0, 99>),
    /// Operator 5's KEYBOARD LEVEL SCALE LEFT CURVE: voice parameter 32.
    op5_left_curve(U8In<0, 3>),
    /// Operator 5's KEYBOARD LEVEL SCALE RIGHT CURVE: voice parameter 33.
    op5_right_curve(U8In<0, 3>),
    /// Operator 5's KEYBOARD RATE SCALING: voice parameter 34.
    op5_rate_scaling(U8In<0, 7>),
    /// Operator 5's MOD SENSITIVITY AMPLITUDE: voice parameter 35.
    op5_amplitude_sensitivity(U8In<0, 3>),
    /// Operator 5's OPERATOR KEY VELOCITY SENSITIVITY: voice parameter 36.
    op5_velocity_sensitivity(U8In<0, 7>),
    /// Operator 5's OPERATOR OUTPUT LEVEL: voice parameter 37.
    op5_output_level(U8In<0, 99>),
    /// Operator 5's OSCILLATOR MODE: voice parameter 38.
    op5_oscillator_mode(U8In<0, 1>),
    /// Operator 5's OSCILLATOR FREQUENCY COARSE: voice parameter 39.
    op5_frequency_coarse(U8In<0, 31>),
    /// Operator 5's OSCILLATOR FREQUENCY FINE: voice parameter 40.
    op5_frequency_fine(U8In<0, 99>),
    /// Operator 5's DETUNE: voice parameter 41.
    op5_detune(U8In<0, 14>),
    /// Operator 6's EG RATE 1: voice parameter 0.
    op6_eg_rate_1(U8In<0, 99>),
    /// Operator 6's EG RATE 2: voice parameter 1.
    op6_eg_rate_2(U8In<0, 99>),
    /// Operator 6's EG RATE 3: voice parameter 2.
    op6_eg_rate_3(U8In<0, 99>),
    /// Operator 6's EG RATE 4: voice parameter 3.
    op6_eg_rate_4(U8In<0, 99>),
    /// Operator 6's EG LEVEL 1: voice parameter 4.
    op6_eg_level_1(U8In<0, 99>),
    /// Operator 6's EG LEVEL 2: voice parameter 5.
    op6_eg_level_2(U8In<0, 99>),
    /// Operator 6's EG LEVEL 3: voice parameter 6.
    op6_eg_level_3(U8In<0, 99>),
    /// Operator 6's EG LEVEL 4: voice parameter 7.
    op6_eg_level_4(U8In<0, 99>),
    /// Operator 6's KEYBOARD LEVEL SCALE BREAK POINT: voice parameter 8.
    op6_break_point(U8In<0, 99>),
    /// Operator 6's KEYBOARD LEVEL SCALE LEFT DEPTH: voice parameter 9.
    op6_left_depth(U8In<0, 99>),
    /// Operator 6's KEYBOARD LEVEL SCALE RIGHT DEPTH: voice parameter 10.
    op6_right_depth(U8In<0, 99>),
    /// Operator 6's KEYBOARD LEVEL SCALE LEFT CURVE: voice parameter 11.
    op6_left_curve(U8In<0, 3>),
    /// Operator 6's KEYBOARD LEVEL SCALE RIGHT CURVE: voice parameter 12.
    op6_right_curve(U8In<0, 3>),
    /// Operator 6's KEYBOARD RATE SCALING: voice parameter 13.
    op6_rate_scaling(U8In<0, 7>),
    /// Operator 6's MOD SENSITIVITY AMPLITUDE: voice parameter 14.
    op6_amplitude_sensitivity(U8In<0, 3>),
    /// Operator 6's OPERATOR KEY VELOCITY SENSITIVITY: voice parameter 15.
    op6_velocity_sensitivity(U8In<0, 7>),
    /// Operator 6's OPERATOR OUTPUT LEVEL: voice parameter 16.
    op6_output_level(U8In<0, 99>),
    /// Operator 6's OSCILLATOR MODE: voice parameter 17.
    op6_oscillator_mode(U8In<0, 1>),
    /// Operator 6's OSCILLATOR FREQUENCY COARSE: voice parameter 18.
    op6_frequency_coarse(U8In<0, 31>),
    /// Operator 6's OSCILLATOR FREQUENCY FINE: voice parameter 19.
    op6_frequency_fine(U8In<0, 99>),
    /// Operator 6's DETUNE: voice parameter 20.
    op6_detune(U8In<0, 14>),
    /// PITCH EG RATE 1: voice parameter 126.
    pitch_eg_rate_1(U8In<0, 99>),
    /// PITCH EG RATE 2: voice parameter 127.
    pitch_eg_rate_2(U8In<0, 99>),
    /// PITCH EG RATE 3: voice parameter 128.
    pitch_eg_rate_3(U8In<0, 99>),
    /// PITCH EG RATE 4: voice parameter 129.
    pitch_eg_rate_4(U8In<0, 99>),
    /// PITCH EG LEVEL 1: voice parameter 130.
    pitch_eg_level_1(U8In<0, 99>),
    /// PITCH EG LEVEL 2: voice parameter 131.
    pitch_eg_level_2(U8In<0, 99>),
    /// PITCH EG LEVEL 3: voice parameter 132.
    pitch_eg_level_3(U8In<0, 99>),
    /// PITCH EG LEVEL 4: voice parameter 133.
    pitch_eg_level_4(U8In<0, 99>),
    /// ALGORITHM SELECT, 0 to 31: 0 is the algorithm the instrument shows as 1: voice parameter 134.
    algorithm(U8In<0, 31>),
    /// FEED BACK, 0 to 7: voice parameter 135.
    feedback(U8In<0, 7>),
    /// OSCILLATOR SYNC, 0 or 1: voice parameter 136.
    oscillator_sync(U8In<0, 1>),
    /// LFO SPEED, 0 to 99: voice parameter 137.
    lfo_speed(U8In<0, 99>),
    /// LFO DELAY, 0 to 99: voice parameter 138.
    lfo_delay(U8In<0, 99>),
    /// LFO PMD, 0 to 99: voice parameter 139.
    lfo_pitch_depth(U8In<0, 99>),
    /// LFO AMD, 0 to 99: voice parameter 140.
    lfo_amplitude_depth(U8In<0, 99>),
    /// LFO SYNC, 0 or 1: voice parameter 141.
    lfo_sync(U8In<0, 1>),
    /// LFO WAVE: 0 triangle, 1 sawtooth down, 2 sawtooth up, 3 square, 4 sine, 5 sample and hold: voice parameter 142.
    lfo_wave(U8In<0, 5>),
    /// MOD SENSITIVITY PITCH, 0 to 7: voice parameter 143.
    pitch_sensitivity(U8In<0, 7>),
    /// TRANSPOSE, 0 to 48, 24 none: voice parameter 144.
    transpose(U8In<0, 48>),
    /// VOICE NAME 1, 0 to 127: voice parameter 145.
    name_1(U8In<0, 127>),
    /// VOICE NAME 2, 0 to 127: voice parameter 146.
    name_2(U8In<0, 127>),
    /// VOICE NAME 3, 0 to 127: voice parameter 147.
    name_3(U8In<0, 127>),
    /// VOICE NAME 4, 0 to 127: voice parameter 148.
    name_4(U8In<0, 127>),
    /// VOICE NAME 5, 0 to 127: voice parameter 149.
    name_5(U8In<0, 127>),
    /// VOICE NAME 6, 0 to 127: voice parameter 150.
    name_6(U8In<0, 127>),
    /// VOICE NAME 7, 0 to 127: voice parameter 151.
    name_7(U8In<0, 127>),
    /// VOICE NAME 8, 0 to 127: voice parameter 152.
    name_8(U8In<0, 127>),
    /// VOICE NAME 9, 0 to 127: voice parameter 153.
    name_9(U8In<0, 127>),
    /// VOICE NAME 10, 0 to 127: voice parameter 154.
    name_10(U8In<0, 127>),
    /// OPERATOR ON/OFF, one bit an operator, 0 to 63: voice parameter 155.
    operator_switch(U8In<0, 63>),
    /// MONO/POLY MODE, 0 to 1: function parameter 64 (S7-33).
    mono_poly(U8In<0, 1>),
    /// PITCH BEND RANGE, 0 to 12: function parameter 65 (S7-33).
    pitch_bend_range(U8In<0, 12>),
    /// PITCH BEND STEP, 0 to 12: function parameter 66 (S7-33).
    pitch_bend_step(U8In<0, 12>),
    /// PORTAMENTO MODE, 0 to 1: function parameter 67 (S7-33).
    portamento_mode(U8In<0, 1>),
    /// PORTAMENTO/GLISSANDO, 0 to 1: function parameter 68 (S7-33).
    glissando(U8In<0, 1>),
    /// PORTAMENTO TIME, 0 to 99: function parameter 69 (S7-33).
    portamento_time(U8In<0, 99>),
    /// MODULATION WHEEL RANGE, 0 to 99: function parameter 70 (S7-33).
    modulation_wheel_range(U8In<0, 99>),
    /// MODULATION WHEEL ASSIGN, 0 to 7: function parameter 71 (S7-33).
    modulation_wheel_assign(U8In<0, 7>),
    /// FOOT CONTROLLER RANGE, 0 to 99: function parameter 72 (S7-33).
    foot_controller_range(U8In<0, 99>),
    /// FOOT CONTROLLER ASSIGN, 0 to 7: function parameter 73 (S7-33).
    foot_controller_assign(U8In<0, 7>),
    /// BREATH CONTROLLER RANGE, 0 to 99: function parameter 74 (S7-33).
    breath_controller_range(U8In<0, 99>),
    /// BREATH CONTROLLER ASSIGN, 0 to 7: function parameter 75 (S7-33).
    breath_controller_assign(U8In<0, 7>),
    /// AFTER TOUCH RANGE, 0 to 99: function parameter 76 (S7-33).
    after_touch_range(U8In<0, 99>),
    /// AFTER TOUCH ASSIGN, 0 to 7: function parameter 77 (S7-33).
    after_touch_assign(U8In<0, 7>),
}

impl Synth7Parameter {
    /// Every parameter's name, in the order of the family document's table.
    pub const NAMES: [&'static str; 170] = [
        "op1_eg_rate_1",
        "op1_eg_rate_2",
        "op1_eg_rate_3",
        "op1_eg_rate_4",
        "op1_eg_level_1",
        "op1_eg_level_2",
        "op1_eg_level_3",
        "op1_eg_level_4",
        "op1_break_point",
        "op1_left_depth",
        "op1_right_depth",
        "op1_left_curve",
        "op1_right_curve",
        "op1_rate_scaling",
        "op1_amplitude_sensitivity",
        "op1_velocity_sensitivity",
        "op1_output_level",
        "op1_oscillator_mode",
        "op1_frequency_coarse",
        "op1_frequency_fine",
        "op1_detune",
        "op2_eg_rate_1",
        "op2_eg_rate_2",
        "op2_eg_rate_3",
        "op2_eg_rate_4",
        "op2_eg_level_1",
        "op2_eg_level_2",
        "op2_eg_level_3",
        "op2_eg_level_4",
        "op2_break_point",
        "op2_left_depth",
        "op2_right_depth",
        "op2_left_curve",
        "op2_right_curve",
        "op2_rate_scaling",
        "op2_amplitude_sensitivity",
        "op2_velocity_sensitivity",
        "op2_output_level",
        "op2_oscillator_mode",
        "op2_frequency_coarse",
        "op2_frequency_fine",
        "op2_detune",
        "op3_eg_rate_1",
        "op3_eg_rate_2",
        "op3_eg_rate_3",
        "op3_eg_rate_4",
        "op3_eg_level_1",
        "op3_eg_level_2",
        "op3_eg_level_3",
        "op3_eg_level_4",
        "op3_break_point",
        "op3_left_depth",
        "op3_right_depth",
        "op3_left_curve",
        "op3_right_curve",
        "op3_rate_scaling",
        "op3_amplitude_sensitivity",
        "op3_velocity_sensitivity",
        "op3_output_level",
        "op3_oscillator_mode",
        "op3_frequency_coarse",
        "op3_frequency_fine",
        "op3_detune",
        "op4_eg_rate_1",
        "op4_eg_rate_2",
        "op4_eg_rate_3",
        "op4_eg_rate_4",
        "op4_eg_level_1",
        "op4_eg_level_2",
        "op4_eg_level_3",
        "op4_eg_level_4",
        "op4_break_point",
        "op4_left_depth",
        "op4_right_depth",
        "op4_left_curve",
        "op4_right_curve",
        "op4_rate_scaling",
        "op4_amplitude_sensitivity",
        "op4_velocity_sensitivity",
        "op4_output_level",
        "op4_oscillator_mode",
        "op4_frequency_coarse",
        "op4_frequency_fine",
        "op4_detune",
        "op5_eg_rate_1",
        "op5_eg_rate_2",
        "op5_eg_rate_3",
        "op5_eg_rate_4",
        "op5_eg_level_1",
        "op5_eg_level_2",
        "op5_eg_level_3",
        "op5_eg_level_4",
        "op5_break_point",
        "op5_left_depth",
        "op5_right_depth",
        "op5_left_curve",
        "op5_right_curve",
        "op5_rate_scaling",
        "op5_amplitude_sensitivity",
        "op5_velocity_sensitivity",
        "op5_output_level",
        "op5_oscillator_mode",
        "op5_frequency_coarse",
        "op5_frequency_fine",
        "op5_detune",
        "op6_eg_rate_1",
        "op6_eg_rate_2",
        "op6_eg_rate_3",
        "op6_eg_rate_4",
        "op6_eg_level_1",
        "op6_eg_level_2",
        "op6_eg_level_3",
        "op6_eg_level_4",
        "op6_break_point",
        "op6_left_depth",
        "op6_right_depth",
        "op6_left_curve",
        "op6_right_curve",
        "op6_rate_scaling",
        "op6_amplitude_sensitivity",
        "op6_velocity_sensitivity",
        "op6_output_level",
        "op6_oscillator_mode",
        "op6_frequency_coarse",
        "op6_frequency_fine",
        "op6_detune",
        "pitch_eg_rate_1",
        "pitch_eg_rate_2",
        "pitch_eg_rate_3",
        "pitch_eg_rate_4",
        "pitch_eg_level_1",
        "pitch_eg_level_2",
        "pitch_eg_level_3",
        "pitch_eg_level_4",
        "algorithm",
        "feedback",
        "oscillator_sync",
        "lfo_speed",
        "lfo_delay",
        "lfo_pitch_depth",
        "lfo_amplitude_depth",
        "lfo_sync",
        "lfo_wave",
        "pitch_sensitivity",
        "transpose",
        "name_1",
        "name_2",
        "name_3",
        "name_4",
        "name_5",
        "name_6",
        "name_7",
        "name_8",
        "name_9",
        "name_10",
        "operator_switch",
        "mono_poly",
        "pitch_bend_range",
        "pitch_bend_step",
        "portamento_mode",
        "glissando",
        "portamento_time",
        "modulation_wheel_range",
        "modulation_wheel_assign",
        "foot_controller_range",
        "foot_controller_assign",
        "breath_controller_range",
        "breath_controller_assign",
        "after_touch_range",
        "after_touch_assign",
    ];

    /// The parameter's name.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one arm for each of the family's named parameters")]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::op1_eg_rate_1(_) => "op1_eg_rate_1",
            Self::op1_eg_rate_2(_) => "op1_eg_rate_2",
            Self::op1_eg_rate_3(_) => "op1_eg_rate_3",
            Self::op1_eg_rate_4(_) => "op1_eg_rate_4",
            Self::op1_eg_level_1(_) => "op1_eg_level_1",
            Self::op1_eg_level_2(_) => "op1_eg_level_2",
            Self::op1_eg_level_3(_) => "op1_eg_level_3",
            Self::op1_eg_level_4(_) => "op1_eg_level_4",
            Self::op1_break_point(_) => "op1_break_point",
            Self::op1_left_depth(_) => "op1_left_depth",
            Self::op1_right_depth(_) => "op1_right_depth",
            Self::op1_left_curve(_) => "op1_left_curve",
            Self::op1_right_curve(_) => "op1_right_curve",
            Self::op1_rate_scaling(_) => "op1_rate_scaling",
            Self::op1_amplitude_sensitivity(_) => "op1_amplitude_sensitivity",
            Self::op1_velocity_sensitivity(_) => "op1_velocity_sensitivity",
            Self::op1_output_level(_) => "op1_output_level",
            Self::op1_oscillator_mode(_) => "op1_oscillator_mode",
            Self::op1_frequency_coarse(_) => "op1_frequency_coarse",
            Self::op1_frequency_fine(_) => "op1_frequency_fine",
            Self::op1_detune(_) => "op1_detune",
            Self::op2_eg_rate_1(_) => "op2_eg_rate_1",
            Self::op2_eg_rate_2(_) => "op2_eg_rate_2",
            Self::op2_eg_rate_3(_) => "op2_eg_rate_3",
            Self::op2_eg_rate_4(_) => "op2_eg_rate_4",
            Self::op2_eg_level_1(_) => "op2_eg_level_1",
            Self::op2_eg_level_2(_) => "op2_eg_level_2",
            Self::op2_eg_level_3(_) => "op2_eg_level_3",
            Self::op2_eg_level_4(_) => "op2_eg_level_4",
            Self::op2_break_point(_) => "op2_break_point",
            Self::op2_left_depth(_) => "op2_left_depth",
            Self::op2_right_depth(_) => "op2_right_depth",
            Self::op2_left_curve(_) => "op2_left_curve",
            Self::op2_right_curve(_) => "op2_right_curve",
            Self::op2_rate_scaling(_) => "op2_rate_scaling",
            Self::op2_amplitude_sensitivity(_) => "op2_amplitude_sensitivity",
            Self::op2_velocity_sensitivity(_) => "op2_velocity_sensitivity",
            Self::op2_output_level(_) => "op2_output_level",
            Self::op2_oscillator_mode(_) => "op2_oscillator_mode",
            Self::op2_frequency_coarse(_) => "op2_frequency_coarse",
            Self::op2_frequency_fine(_) => "op2_frequency_fine",
            Self::op2_detune(_) => "op2_detune",
            Self::op3_eg_rate_1(_) => "op3_eg_rate_1",
            Self::op3_eg_rate_2(_) => "op3_eg_rate_2",
            Self::op3_eg_rate_3(_) => "op3_eg_rate_3",
            Self::op3_eg_rate_4(_) => "op3_eg_rate_4",
            Self::op3_eg_level_1(_) => "op3_eg_level_1",
            Self::op3_eg_level_2(_) => "op3_eg_level_2",
            Self::op3_eg_level_3(_) => "op3_eg_level_3",
            Self::op3_eg_level_4(_) => "op3_eg_level_4",
            Self::op3_break_point(_) => "op3_break_point",
            Self::op3_left_depth(_) => "op3_left_depth",
            Self::op3_right_depth(_) => "op3_right_depth",
            Self::op3_left_curve(_) => "op3_left_curve",
            Self::op3_right_curve(_) => "op3_right_curve",
            Self::op3_rate_scaling(_) => "op3_rate_scaling",
            Self::op3_amplitude_sensitivity(_) => "op3_amplitude_sensitivity",
            Self::op3_velocity_sensitivity(_) => "op3_velocity_sensitivity",
            Self::op3_output_level(_) => "op3_output_level",
            Self::op3_oscillator_mode(_) => "op3_oscillator_mode",
            Self::op3_frequency_coarse(_) => "op3_frequency_coarse",
            Self::op3_frequency_fine(_) => "op3_frequency_fine",
            Self::op3_detune(_) => "op3_detune",
            Self::op4_eg_rate_1(_) => "op4_eg_rate_1",
            Self::op4_eg_rate_2(_) => "op4_eg_rate_2",
            Self::op4_eg_rate_3(_) => "op4_eg_rate_3",
            Self::op4_eg_rate_4(_) => "op4_eg_rate_4",
            Self::op4_eg_level_1(_) => "op4_eg_level_1",
            Self::op4_eg_level_2(_) => "op4_eg_level_2",
            Self::op4_eg_level_3(_) => "op4_eg_level_3",
            Self::op4_eg_level_4(_) => "op4_eg_level_4",
            Self::op4_break_point(_) => "op4_break_point",
            Self::op4_left_depth(_) => "op4_left_depth",
            Self::op4_right_depth(_) => "op4_right_depth",
            Self::op4_left_curve(_) => "op4_left_curve",
            Self::op4_right_curve(_) => "op4_right_curve",
            Self::op4_rate_scaling(_) => "op4_rate_scaling",
            Self::op4_amplitude_sensitivity(_) => "op4_amplitude_sensitivity",
            Self::op4_velocity_sensitivity(_) => "op4_velocity_sensitivity",
            Self::op4_output_level(_) => "op4_output_level",
            Self::op4_oscillator_mode(_) => "op4_oscillator_mode",
            Self::op4_frequency_coarse(_) => "op4_frequency_coarse",
            Self::op4_frequency_fine(_) => "op4_frequency_fine",
            Self::op4_detune(_) => "op4_detune",
            Self::op5_eg_rate_1(_) => "op5_eg_rate_1",
            Self::op5_eg_rate_2(_) => "op5_eg_rate_2",
            Self::op5_eg_rate_3(_) => "op5_eg_rate_3",
            Self::op5_eg_rate_4(_) => "op5_eg_rate_4",
            Self::op5_eg_level_1(_) => "op5_eg_level_1",
            Self::op5_eg_level_2(_) => "op5_eg_level_2",
            Self::op5_eg_level_3(_) => "op5_eg_level_3",
            Self::op5_eg_level_4(_) => "op5_eg_level_4",
            Self::op5_break_point(_) => "op5_break_point",
            Self::op5_left_depth(_) => "op5_left_depth",
            Self::op5_right_depth(_) => "op5_right_depth",
            Self::op5_left_curve(_) => "op5_left_curve",
            Self::op5_right_curve(_) => "op5_right_curve",
            Self::op5_rate_scaling(_) => "op5_rate_scaling",
            Self::op5_amplitude_sensitivity(_) => "op5_amplitude_sensitivity",
            Self::op5_velocity_sensitivity(_) => "op5_velocity_sensitivity",
            Self::op5_output_level(_) => "op5_output_level",
            Self::op5_oscillator_mode(_) => "op5_oscillator_mode",
            Self::op5_frequency_coarse(_) => "op5_frequency_coarse",
            Self::op5_frequency_fine(_) => "op5_frequency_fine",
            Self::op5_detune(_) => "op5_detune",
            Self::op6_eg_rate_1(_) => "op6_eg_rate_1",
            Self::op6_eg_rate_2(_) => "op6_eg_rate_2",
            Self::op6_eg_rate_3(_) => "op6_eg_rate_3",
            Self::op6_eg_rate_4(_) => "op6_eg_rate_4",
            Self::op6_eg_level_1(_) => "op6_eg_level_1",
            Self::op6_eg_level_2(_) => "op6_eg_level_2",
            Self::op6_eg_level_3(_) => "op6_eg_level_3",
            Self::op6_eg_level_4(_) => "op6_eg_level_4",
            Self::op6_break_point(_) => "op6_break_point",
            Self::op6_left_depth(_) => "op6_left_depth",
            Self::op6_right_depth(_) => "op6_right_depth",
            Self::op6_left_curve(_) => "op6_left_curve",
            Self::op6_right_curve(_) => "op6_right_curve",
            Self::op6_rate_scaling(_) => "op6_rate_scaling",
            Self::op6_amplitude_sensitivity(_) => "op6_amplitude_sensitivity",
            Self::op6_velocity_sensitivity(_) => "op6_velocity_sensitivity",
            Self::op6_output_level(_) => "op6_output_level",
            Self::op6_oscillator_mode(_) => "op6_oscillator_mode",
            Self::op6_frequency_coarse(_) => "op6_frequency_coarse",
            Self::op6_frequency_fine(_) => "op6_frequency_fine",
            Self::op6_detune(_) => "op6_detune",
            Self::pitch_eg_rate_1(_) => "pitch_eg_rate_1",
            Self::pitch_eg_rate_2(_) => "pitch_eg_rate_2",
            Self::pitch_eg_rate_3(_) => "pitch_eg_rate_3",
            Self::pitch_eg_rate_4(_) => "pitch_eg_rate_4",
            Self::pitch_eg_level_1(_) => "pitch_eg_level_1",
            Self::pitch_eg_level_2(_) => "pitch_eg_level_2",
            Self::pitch_eg_level_3(_) => "pitch_eg_level_3",
            Self::pitch_eg_level_4(_) => "pitch_eg_level_4",
            Self::algorithm(_) => "algorithm",
            Self::feedback(_) => "feedback",
            Self::oscillator_sync(_) => "oscillator_sync",
            Self::lfo_speed(_) => "lfo_speed",
            Self::lfo_delay(_) => "lfo_delay",
            Self::lfo_pitch_depth(_) => "lfo_pitch_depth",
            Self::lfo_amplitude_depth(_) => "lfo_amplitude_depth",
            Self::lfo_sync(_) => "lfo_sync",
            Self::lfo_wave(_) => "lfo_wave",
            Self::pitch_sensitivity(_) => "pitch_sensitivity",
            Self::transpose(_) => "transpose",
            Self::name_1(_) => "name_1",
            Self::name_2(_) => "name_2",
            Self::name_3(_) => "name_3",
            Self::name_4(_) => "name_4",
            Self::name_5(_) => "name_5",
            Self::name_6(_) => "name_6",
            Self::name_7(_) => "name_7",
            Self::name_8(_) => "name_8",
            Self::name_9(_) => "name_9",
            Self::name_10(_) => "name_10",
            Self::operator_switch(_) => "operator_switch",
            Self::mono_poly(_) => "mono_poly",
            Self::pitch_bend_range(_) => "pitch_bend_range",
            Self::pitch_bend_step(_) => "pitch_bend_step",
            Self::portamento_mode(_) => "portamento_mode",
            Self::glissando(_) => "glissando",
            Self::portamento_time(_) => "portamento_time",
            Self::modulation_wheel_range(_) => "modulation_wheel_range",
            Self::modulation_wheel_assign(_) => "modulation_wheel_assign",
            Self::foot_controller_range(_) => "foot_controller_range",
            Self::foot_controller_assign(_) => "foot_controller_assign",
            Self::breath_controller_range(_) => "breath_controller_range",
            Self::breath_controller_assign(_) => "breath_controller_assign",
            Self::after_touch_range(_) => "after_touch_range",
            Self::after_touch_assign(_) => "after_touch_assign",
        }
    }
}

/// A synth106 patch as named fields: the sixteen sliders and the switches of
/// synth106-synth60.md, S106-22 and S106-36 (server.md SRV-30).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth106Panel {
    /// LFO rate, 0 to 127 (Juno-106 parameter 0).
    pub lfo_rate: U8In<0, 127>,
    /// LFO delay, 0 to 127 (Juno-106 parameter 1).
    pub lfo_delay: U8In<0, 127>,
    /// DCO LFO, 0 to 127 (Juno-106 parameter 2).
    pub dco_lfo: U8In<0, 127>,
    /// DCO PWM, 0 to 127 (Juno-106 parameter 3).
    pub dco_pwm: U8In<0, 127>,
    /// noise level, 0 to 127 (Juno-106 parameter 4).
    pub noise_level: U8In<0, 127>,
    /// VCF cutoff, 0 to 127 (Juno-106 parameter 5).
    pub vcf_frequency: U8In<0, 127>,
    /// resonance, 0 to 127 (Juno-106 parameter 6).
    pub vcf_resonance: U8In<0, 127>,
    /// VCF ENV, 0 to 127 (Juno-106 parameter 7).
    pub vcf_envelope: U8In<0, 127>,
    /// VCF LFO, 0 to 127 (Juno-106 parameter 8).
    pub vcf_lfo: U8In<0, 127>,
    /// VCF KYBD, 0 to 127 (Juno-106 parameter 9).
    pub vcf_keyboard: U8In<0, 127>,
    /// VCA level, 0 to 127 (Juno-106 parameter 10).
    pub vca_level: U8In<0, 127>,
    /// attack, 0 to 127 (Juno-106 parameter 11).
    pub attack: U8In<0, 127>,
    /// decay, 0 to 127 (Juno-106 parameter 12).
    pub decay: U8In<0, 127>,
    /// sustain, 0 to 127 (Juno-106 parameter 13).
    pub sustain: U8In<0, 127>,
    /// release, 0 to 127 (Juno-106 parameter 14).
    pub release: U8In<0, 127>,
    /// sub level, 0 to 127 (Juno-106 parameter 15).
    pub sub_level: U8In<0, 127>,
    /// The range switch (byte 16, bits 2 to 0); on synth60 the octave transpose switch (S106-22).
    pub range: Synth106Range,
    /// The pulse waveform (byte 16, bit 3).
    pub pulse: bool,
    /// The sawtooth waveform (byte 16, bit 4).
    pub sawtooth: bool,
    /// The chorus (byte 16, bits 6 and 5).
    pub chorus: Synth106Chorus,
    /// The PWM source (byte 17, bit 0).
    pub pwm: Synth106Pwm,
    /// The VCF envelope's polarity (byte 17, bit 1).
    pub envelope_polarity: Synth106Polarity,
    /// The VCA's control (byte 17, bit 2).
    pub vca: Synth106Vca,
    /// The high-pass filter switch's position, 0 to 3 (byte 17, bits 4 and 3 hold 3 - position).
    pub hpf: U8In<0, 3>,
}

/// A synth60 patch as named fields: synth106's, with the PWM source also the
/// envelope, the chorus also both, and the sub-oscillator switch (server.md SRV-31).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth60Panel {
    /// LFO rate, 0 to 127 (Juno-106 parameter 0).
    pub lfo_rate: U8In<0, 127>,
    /// LFO delay, 0 to 127 (Juno-106 parameter 1).
    pub lfo_delay: U8In<0, 127>,
    /// DCO LFO, 0 to 127 (Juno-106 parameter 2).
    pub dco_lfo: U8In<0, 127>,
    /// DCO PWM, 0 to 127 (Juno-106 parameter 3).
    pub dco_pwm: U8In<0, 127>,
    /// noise level, 0 to 127 (Juno-106 parameter 4).
    pub noise_level: U8In<0, 127>,
    /// VCF cutoff, 0 to 127 (Juno-106 parameter 5).
    pub vcf_frequency: U8In<0, 127>,
    /// resonance, 0 to 127 (Juno-106 parameter 6).
    pub vcf_resonance: U8In<0, 127>,
    /// VCF ENV, 0 to 127 (Juno-106 parameter 7).
    pub vcf_envelope: U8In<0, 127>,
    /// VCF LFO, 0 to 127 (Juno-106 parameter 8).
    pub vcf_lfo: U8In<0, 127>,
    /// VCF KYBD, 0 to 127 (Juno-106 parameter 9).
    pub vcf_keyboard: U8In<0, 127>,
    /// VCA level, 0 to 127 (Juno-106 parameter 10).
    pub vca_level: U8In<0, 127>,
    /// attack, 0 to 127 (Juno-106 parameter 11).
    pub attack: U8In<0, 127>,
    /// decay, 0 to 127 (Juno-106 parameter 12).
    pub decay: U8In<0, 127>,
    /// sustain, 0 to 127 (Juno-106 parameter 13).
    pub sustain: U8In<0, 127>,
    /// release, 0 to 127 (Juno-106 parameter 14).
    pub release: U8In<0, 127>,
    /// sub level, 0 to 127 (Juno-106 parameter 15).
    pub sub_level: U8In<0, 127>,
    /// The range switch (byte 16, bits 2 to 0); on synth60 the octave transpose switch (S106-22).
    pub range: Synth106Range,
    /// The pulse waveform (byte 16, bit 3).
    pub pulse: bool,
    /// The sawtooth waveform (byte 16, bit 4).
    pub sawtooth: bool,
    /// The chorus (byte 16, bits 6 and 5).
    pub chorus: Synth60Chorus,
    /// The PWM source (byte 17, bit 0).
    pub pwm: Synth60Pwm,
    /// The VCF envelope's polarity (byte 17, bit 1).
    pub envelope_polarity: Synth106Polarity,
    /// The VCA's control (byte 17, bit 2).
    pub vca: Synth106Vca,
    /// The high-pass filter switch's position, 0 to 3 (byte 17, bits 4 and 3 hold 3 - position).
    pub hpf: U8In<0, 3>,
    /// The sub-oscillator switch (JU-06A controller 28, S106-31).
    pub sub_switch: bool,
}

/// One of synth106's named parameters, one for each field of its panel
/// (server.md SRV-33).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(non_camel_case_types, reason = "each variant is the parameter's name on the wire, which the schema and the kits read as written (server.md SRV-22)")]
pub enum Synth106Parameter {
    /// LFO rate, 0 to 127 (Juno-106 parameter 0).
    lfo_rate(U8In<0, 127>),
    /// LFO delay, 0 to 127 (Juno-106 parameter 1).
    lfo_delay(U8In<0, 127>),
    /// DCO LFO, 0 to 127 (Juno-106 parameter 2).
    dco_lfo(U8In<0, 127>),
    /// DCO PWM, 0 to 127 (Juno-106 parameter 3).
    dco_pwm(U8In<0, 127>),
    /// noise level, 0 to 127 (Juno-106 parameter 4).
    noise_level(U8In<0, 127>),
    /// VCF cutoff, 0 to 127 (Juno-106 parameter 5).
    vcf_frequency(U8In<0, 127>),
    /// resonance, 0 to 127 (Juno-106 parameter 6).
    vcf_resonance(U8In<0, 127>),
    /// VCF ENV, 0 to 127 (Juno-106 parameter 7).
    vcf_envelope(U8In<0, 127>),
    /// VCF LFO, 0 to 127 (Juno-106 parameter 8).
    vcf_lfo(U8In<0, 127>),
    /// VCF KYBD, 0 to 127 (Juno-106 parameter 9).
    vcf_keyboard(U8In<0, 127>),
    /// VCA level, 0 to 127 (Juno-106 parameter 10).
    vca_level(U8In<0, 127>),
    /// attack, 0 to 127 (Juno-106 parameter 11).
    attack(U8In<0, 127>),
    /// decay, 0 to 127 (Juno-106 parameter 12).
    decay(U8In<0, 127>),
    /// sustain, 0 to 127 (Juno-106 parameter 13).
    sustain(U8In<0, 127>),
    /// release, 0 to 127 (Juno-106 parameter 14).
    release(U8In<0, 127>),
    /// sub level, 0 to 127 (Juno-106 parameter 15).
    sub_level(U8In<0, 127>),
    /// The range switch (byte 16, bits 2 to 0); on synth60 the octave transpose switch (S106-22).
    range(Synth106Range),
    /// The pulse waveform (byte 16, bit 3).
    pulse(bool),
    /// The sawtooth waveform (byte 16, bit 4).
    sawtooth(bool),
    /// The chorus (byte 16, bits 6 and 5).
    chorus(Synth106Chorus),
    /// The PWM source (byte 17, bit 0).
    pwm(Synth106Pwm),
    /// The VCF envelope's polarity (byte 17, bit 1).
    envelope_polarity(Synth106Polarity),
    /// The VCA's control (byte 17, bit 2).
    vca(Synth106Vca),
    /// The high-pass filter switch's position, 0 to 3 (byte 17, bits 4 and 3 hold 3 - position).
    hpf(U8In<0, 3>),
}

impl Synth106Parameter {
    /// Every parameter's name, in the order of the family document's table.
    pub const NAMES: [&'static str; 24] = [
        "lfo_rate",
        "lfo_delay",
        "dco_lfo",
        "dco_pwm",
        "noise_level",
        "vcf_frequency",
        "vcf_resonance",
        "vcf_envelope",
        "vcf_lfo",
        "vcf_keyboard",
        "vca_level",
        "attack",
        "decay",
        "sustain",
        "release",
        "sub_level",
        "range",
        "pulse",
        "sawtooth",
        "chorus",
        "pwm",
        "envelope_polarity",
        "vca",
        "hpf",
    ];

    /// The parameter's name.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one arm for each of the family's named parameters")]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::lfo_rate(_) => "lfo_rate",
            Self::lfo_delay(_) => "lfo_delay",
            Self::dco_lfo(_) => "dco_lfo",
            Self::dco_pwm(_) => "dco_pwm",
            Self::noise_level(_) => "noise_level",
            Self::vcf_frequency(_) => "vcf_frequency",
            Self::vcf_resonance(_) => "vcf_resonance",
            Self::vcf_envelope(_) => "vcf_envelope",
            Self::vcf_lfo(_) => "vcf_lfo",
            Self::vcf_keyboard(_) => "vcf_keyboard",
            Self::vca_level(_) => "vca_level",
            Self::attack(_) => "attack",
            Self::decay(_) => "decay",
            Self::sustain(_) => "sustain",
            Self::release(_) => "release",
            Self::sub_level(_) => "sub_level",
            Self::range(_) => "range",
            Self::pulse(_) => "pulse",
            Self::sawtooth(_) => "sawtooth",
            Self::chorus(_) => "chorus",
            Self::pwm(_) => "pwm",
            Self::envelope_polarity(_) => "envelope_polarity",
            Self::vca(_) => "vca",
            Self::hpf(_) => "hpf",
        }
    }
}

/// One of synth60's named parameters, one for each field of its panel
/// (server.md SRV-33).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(non_camel_case_types, reason = "each variant is the parameter's name on the wire, which the schema and the kits read as written (server.md SRV-22)")]
pub enum Synth60Parameter {
    /// LFO rate, 0 to 127 (Juno-106 parameter 0).
    lfo_rate(U8In<0, 127>),
    /// LFO delay, 0 to 127 (Juno-106 parameter 1).
    lfo_delay(U8In<0, 127>),
    /// DCO LFO, 0 to 127 (Juno-106 parameter 2).
    dco_lfo(U8In<0, 127>),
    /// DCO PWM, 0 to 127 (Juno-106 parameter 3).
    dco_pwm(U8In<0, 127>),
    /// noise level, 0 to 127 (Juno-106 parameter 4).
    noise_level(U8In<0, 127>),
    /// VCF cutoff, 0 to 127 (Juno-106 parameter 5).
    vcf_frequency(U8In<0, 127>),
    /// resonance, 0 to 127 (Juno-106 parameter 6).
    vcf_resonance(U8In<0, 127>),
    /// VCF ENV, 0 to 127 (Juno-106 parameter 7).
    vcf_envelope(U8In<0, 127>),
    /// VCF LFO, 0 to 127 (Juno-106 parameter 8).
    vcf_lfo(U8In<0, 127>),
    /// VCF KYBD, 0 to 127 (Juno-106 parameter 9).
    vcf_keyboard(U8In<0, 127>),
    /// VCA level, 0 to 127 (Juno-106 parameter 10).
    vca_level(U8In<0, 127>),
    /// attack, 0 to 127 (Juno-106 parameter 11).
    attack(U8In<0, 127>),
    /// decay, 0 to 127 (Juno-106 parameter 12).
    decay(U8In<0, 127>),
    /// sustain, 0 to 127 (Juno-106 parameter 13).
    sustain(U8In<0, 127>),
    /// release, 0 to 127 (Juno-106 parameter 14).
    release(U8In<0, 127>),
    /// sub level, 0 to 127 (Juno-106 parameter 15).
    sub_level(U8In<0, 127>),
    /// The range switch (byte 16, bits 2 to 0); on synth60 the octave transpose switch (S106-22).
    range(Synth106Range),
    /// The pulse waveform (byte 16, bit 3).
    pulse(bool),
    /// The sawtooth waveform (byte 16, bit 4).
    sawtooth(bool),
    /// The chorus (byte 16, bits 6 and 5).
    chorus(Synth60Chorus),
    /// The PWM source (byte 17, bit 0).
    pwm(Synth60Pwm),
    /// The VCF envelope's polarity (byte 17, bit 1).
    envelope_polarity(Synth106Polarity),
    /// The VCA's control (byte 17, bit 2).
    vca(Synth106Vca),
    /// The high-pass filter switch's position, 0 to 3 (byte 17, bits 4 and 3 hold 3 - position).
    hpf(U8In<0, 3>),
    /// The sub-oscillator switch (JU-06A controller 28, S106-31).
    sub_switch(bool),
}

impl Synth60Parameter {
    /// Every parameter's name, in the order of the family document's table.
    pub const NAMES: [&'static str; 25] = [
        "lfo_rate",
        "lfo_delay",
        "dco_lfo",
        "dco_pwm",
        "noise_level",
        "vcf_frequency",
        "vcf_resonance",
        "vcf_envelope",
        "vcf_lfo",
        "vcf_keyboard",
        "vca_level",
        "attack",
        "decay",
        "sustain",
        "release",
        "sub_level",
        "range",
        "pulse",
        "sawtooth",
        "chorus",
        "pwm",
        "envelope_polarity",
        "vca",
        "hpf",
        "sub_switch",
    ];

    /// The parameter's name.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one arm for each of the family's named parameters")]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::lfo_rate(_) => "lfo_rate",
            Self::lfo_delay(_) => "lfo_delay",
            Self::dco_lfo(_) => "dco_lfo",
            Self::dco_pwm(_) => "dco_pwm",
            Self::noise_level(_) => "noise_level",
            Self::vcf_frequency(_) => "vcf_frequency",
            Self::vcf_resonance(_) => "vcf_resonance",
            Self::vcf_envelope(_) => "vcf_envelope",
            Self::vcf_lfo(_) => "vcf_lfo",
            Self::vcf_keyboard(_) => "vcf_keyboard",
            Self::vca_level(_) => "vca_level",
            Self::attack(_) => "attack",
            Self::decay(_) => "decay",
            Self::sustain(_) => "sustain",
            Self::release(_) => "release",
            Self::sub_level(_) => "sub_level",
            Self::range(_) => "range",
            Self::pulse(_) => "pulse",
            Self::sawtooth(_) => "sawtooth",
            Self::chorus(_) => "chorus",
            Self::pwm(_) => "pwm",
            Self::envelope_polarity(_) => "envelope_polarity",
            Self::vca(_) => "vca",
            Self::hpf(_) => "hpf",
            Self::sub_switch(_) => "sub_switch",
        }
    }
}

/// A synthD patch as named fields: the value of each controller or coarse
/// and fine pair of synthd.md, SD-22, in the order of the patch's bytes
/// (server.md SRV-34).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SynthDPanel {
    /// GLIDE (2): controllers 5 and 37, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub glide: U16In<0, 16383>,
    /// Oscillator 2 FREQUENCY (7): controllers 10 and 42, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub oscillator_2_frequency: U16In<0, 16383>,
    /// Oscillator 3 FREQUENCY (10): controllers 12 and 44, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub oscillator_3_frequency: U16In<0, 16383>,
    /// EXTERNAL INPUT VOLUME (15): controllers 14 and 46, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub external_input_volume: U16In<0, 16383>,
    /// Mixer VOLUME of oscillator 1 (12): controllers 15 and 47, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub oscillator_1_volume: U16In<0, 16383>,
    /// Mixer VOLUME of oscillator 2 (13): controllers 16 and 48, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub oscillator_2_volume: U16In<0, 16383>,
    /// Mixer VOLUME of oscillator 3 (14): controllers 17 and 49, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub oscillator_3_volume: U16In<0, 16383>,
    /// NOISE VOLUME (16): controllers 18 and 50, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub noise_volume: U16In<0, 16383>,
    /// CUTOFF FREQUENCY (17): controllers 19 and 51, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub cutoff_frequency: U16In<0, 16383>,
    /// EMPHASIS (18): controllers 21 and 53, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub emphasis: U16In<0, 16383>,
    /// Filter contour ATTACK TIME (20): controllers 23 and 55, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub filter_attack_time: U16In<0, 16383>,
    /// Filter contour DECAY TIME (21): controllers 24 and 56, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub filter_decay_time: U16In<0, 16383>,
    /// Filter contour SUSTAIN LEVEL (22): controllers 25 and 57, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub filter_sustain_level: U16In<0, 16383>,
    /// AMOUNT OF CONTOUR (19): controllers 27 and 59, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub amount_of_contour: U16In<0, 16383>,
    /// Loudness contour ATTACK TIME (23): controllers 28 and 60, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub loudness_attack_time: U16In<0, 16383>,
    /// Loudness contour DECAY TIME (24): controllers 29 and 61, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub loudness_decay_time: U16In<0, 16383>,
    /// Loudness contour SUSTAIN LEVEL (25): controllers 30 and 62, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub loudness_sustain_level: U16In<0, 16383>,
    /// GLIDE switch (38): controller 65, 0 to 127.
    pub glide_switch: U8In<0, 127>,
    /// Oscillator 1 RANGE (4), its octave: controller 74, 0 to 127.
    pub oscillator_1_range_octave: U8In<0, 127>,
    /// Oscillator 2 RANGE (6), its octave: controller 75, 0 to 127.
    pub oscillator_2_range_octave: U8In<0, 127>,
    /// Oscillator 3 RANGE (9), its octave: controller 76, 0 to 127.
    pub oscillator_3_range_octave: U8In<0, 127>,
    /// OSC. 3 CONTROL (28): controller 79, 0 to 127.
    pub oscillator_3_control: U8In<0, 127>,
    /// Oscillator 3 RANGE (9), LO or HI: controller 80, 0 to 127.
    pub oscillator_3_range_lo_hi: U8In<0, 127>,
    /// Mixer switch of the external input (32): controller 81, 0 to 127.
    pub external_input_switch: U8In<0, 127>,
    /// Mixer switch of oscillator 1 (29): controller 82, 0 to 127.
    pub oscillator_1_switch: U8In<0, 127>,
    /// Mixer switch of oscillator 2 (30): controller 83, 0 to 127.
    pub oscillator_2_switch: U8In<0, 127>,
    /// Mixer switch of oscillator 3 (31): controller 84, 0 to 127.
    pub oscillator_3_switch: U8In<0, 127>,
    /// Mixer switch of the noise (33): controller 85, 0 to 127.
    pub noise_switch: U8In<0, 127>,
    /// DECAY switch (39): controller 88, 0 to 127.
    pub decay_switch: U8In<0, 127>,
    /// TUNE (1): controllers 89 and 90, coarse x 128 + fine, 0 to 16,383 (SD-18).
    pub tune: U16In<0, 16383>,
    /// MODULATION MIX (3): controller 102, 0 to 127.
    pub modulation_mix: U8In<0, 127>,
    /// Oscillator 1 RANGE (4), LO or HI: controller 103, 0 to 127.
    pub oscillator_1_range_lo_hi: U8In<0, 127>,
    /// Oscillator 1 WAVEFORM (5): controller 104, 0 to 127.
    pub oscillator_1_waveform: U8In<0, 127>,
    /// Oscillator 2 RANGE (6), LO or HI: controller 105, 0 to 127.
    pub oscillator_2_range_lo_hi: U8In<0, 127>,
    /// Oscillator 2 WAVEFORM (8): controller 106, 0 to 127.
    pub oscillator_2_waveform: U8In<0, 127>,
    /// Oscillator 3 WAVEFORM (11): controller 107, 0 to 127.
    pub oscillator_3_waveform: U8In<0, 127>,
    /// Main output VOLUME (26): controller 108, 0 to 127.
    pub main_output_volume: U8In<0, 127>,
    /// OSCILLATOR MODULATION (27): controller 109, 0 to 127.
    pub oscillator_modulation: U8In<0, 127>,
    /// Noise WHITE / PINK (34): controller 110, 0 to 127.
    pub noise_colour: U8In<0, 127>,
    /// FILTER MODULATION (35): controller 111, 0 to 127.
    pub filter_modulation: U8In<0, 127>,
    /// KEYBOARD CONTROL 1 (36): controller 112, 0 to 127.
    pub keyboard_control_1: U8In<0, 127>,
    /// KEYBOARD CONTROL 2 (37): controller 113, 0 to 127.
    pub keyboard_control_2: U8In<0, 127>,
}

/// One of synthD's named parameters, one for each field of its panel
/// (server.md SRV-36).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(non_camel_case_types, reason = "each variant is the parameter's name on the wire, which the schema and the kits read as written (server.md SRV-22)")]
pub enum SynthDParameter {
    /// GLIDE (2): controllers 5 and 37, coarse x 128 + fine, 0 to 16,383 (SD-18).
    glide(U16In<0, 16383>),
    /// Oscillator 2 FREQUENCY (7): controllers 10 and 42, coarse x 128 + fine, 0 to 16,383 (SD-18).
    oscillator_2_frequency(U16In<0, 16383>),
    /// Oscillator 3 FREQUENCY (10): controllers 12 and 44, coarse x 128 + fine, 0 to 16,383 (SD-18).
    oscillator_3_frequency(U16In<0, 16383>),
    /// EXTERNAL INPUT VOLUME (15): controllers 14 and 46, coarse x 128 + fine, 0 to 16,383 (SD-18).
    external_input_volume(U16In<0, 16383>),
    /// Mixer VOLUME of oscillator 1 (12): controllers 15 and 47, coarse x 128 + fine, 0 to 16,383 (SD-18).
    oscillator_1_volume(U16In<0, 16383>),
    /// Mixer VOLUME of oscillator 2 (13): controllers 16 and 48, coarse x 128 + fine, 0 to 16,383 (SD-18).
    oscillator_2_volume(U16In<0, 16383>),
    /// Mixer VOLUME of oscillator 3 (14): controllers 17 and 49, coarse x 128 + fine, 0 to 16,383 (SD-18).
    oscillator_3_volume(U16In<0, 16383>),
    /// NOISE VOLUME (16): controllers 18 and 50, coarse x 128 + fine, 0 to 16,383 (SD-18).
    noise_volume(U16In<0, 16383>),
    /// CUTOFF FREQUENCY (17): controllers 19 and 51, coarse x 128 + fine, 0 to 16,383 (SD-18).
    cutoff_frequency(U16In<0, 16383>),
    /// EMPHASIS (18): controllers 21 and 53, coarse x 128 + fine, 0 to 16,383 (SD-18).
    emphasis(U16In<0, 16383>),
    /// Filter contour ATTACK TIME (20): controllers 23 and 55, coarse x 128 + fine, 0 to 16,383 (SD-18).
    filter_attack_time(U16In<0, 16383>),
    /// Filter contour DECAY TIME (21): controllers 24 and 56, coarse x 128 + fine, 0 to 16,383 (SD-18).
    filter_decay_time(U16In<0, 16383>),
    /// Filter contour SUSTAIN LEVEL (22): controllers 25 and 57, coarse x 128 + fine, 0 to 16,383 (SD-18).
    filter_sustain_level(U16In<0, 16383>),
    /// AMOUNT OF CONTOUR (19): controllers 27 and 59, coarse x 128 + fine, 0 to 16,383 (SD-18).
    amount_of_contour(U16In<0, 16383>),
    /// Loudness contour ATTACK TIME (23): controllers 28 and 60, coarse x 128 + fine, 0 to 16,383 (SD-18).
    loudness_attack_time(U16In<0, 16383>),
    /// Loudness contour DECAY TIME (24): controllers 29 and 61, coarse x 128 + fine, 0 to 16,383 (SD-18).
    loudness_decay_time(U16In<0, 16383>),
    /// Loudness contour SUSTAIN LEVEL (25): controllers 30 and 62, coarse x 128 + fine, 0 to 16,383 (SD-18).
    loudness_sustain_level(U16In<0, 16383>),
    /// GLIDE switch (38): controller 65, 0 to 127.
    glide_switch(U8In<0, 127>),
    /// Oscillator 1 RANGE (4), its octave: controller 74, 0 to 127.
    oscillator_1_range_octave(U8In<0, 127>),
    /// Oscillator 2 RANGE (6), its octave: controller 75, 0 to 127.
    oscillator_2_range_octave(U8In<0, 127>),
    /// Oscillator 3 RANGE (9), its octave: controller 76, 0 to 127.
    oscillator_3_range_octave(U8In<0, 127>),
    /// OSC. 3 CONTROL (28): controller 79, 0 to 127.
    oscillator_3_control(U8In<0, 127>),
    /// Oscillator 3 RANGE (9), LO or HI: controller 80, 0 to 127.
    oscillator_3_range_lo_hi(U8In<0, 127>),
    /// Mixer switch of the external input (32): controller 81, 0 to 127.
    external_input_switch(U8In<0, 127>),
    /// Mixer switch of oscillator 1 (29): controller 82, 0 to 127.
    oscillator_1_switch(U8In<0, 127>),
    /// Mixer switch of oscillator 2 (30): controller 83, 0 to 127.
    oscillator_2_switch(U8In<0, 127>),
    /// Mixer switch of oscillator 3 (31): controller 84, 0 to 127.
    oscillator_3_switch(U8In<0, 127>),
    /// Mixer switch of the noise (33): controller 85, 0 to 127.
    noise_switch(U8In<0, 127>),
    /// DECAY switch (39): controller 88, 0 to 127.
    decay_switch(U8In<0, 127>),
    /// TUNE (1): controllers 89 and 90, coarse x 128 + fine, 0 to 16,383 (SD-18).
    tune(U16In<0, 16383>),
    /// MODULATION MIX (3): controller 102, 0 to 127.
    modulation_mix(U8In<0, 127>),
    /// Oscillator 1 RANGE (4), LO or HI: controller 103, 0 to 127.
    oscillator_1_range_lo_hi(U8In<0, 127>),
    /// Oscillator 1 WAVEFORM (5): controller 104, 0 to 127.
    oscillator_1_waveform(U8In<0, 127>),
    /// Oscillator 2 RANGE (6), LO or HI: controller 105, 0 to 127.
    oscillator_2_range_lo_hi(U8In<0, 127>),
    /// Oscillator 2 WAVEFORM (8): controller 106, 0 to 127.
    oscillator_2_waveform(U8In<0, 127>),
    /// Oscillator 3 WAVEFORM (11): controller 107, 0 to 127.
    oscillator_3_waveform(U8In<0, 127>),
    /// Main output VOLUME (26): controller 108, 0 to 127.
    main_output_volume(U8In<0, 127>),
    /// OSCILLATOR MODULATION (27): controller 109, 0 to 127.
    oscillator_modulation(U8In<0, 127>),
    /// Noise WHITE / PINK (34): controller 110, 0 to 127.
    noise_colour(U8In<0, 127>),
    /// FILTER MODULATION (35): controller 111, 0 to 127.
    filter_modulation(U8In<0, 127>),
    /// KEYBOARD CONTROL 1 (36): controller 112, 0 to 127.
    keyboard_control_1(U8In<0, 127>),
    /// KEYBOARD CONTROL 2 (37): controller 113, 0 to 127.
    keyboard_control_2(U8In<0, 127>),
}

impl SynthDParameter {
    /// Every parameter's name, in the order of the family document's table.
    pub const NAMES: [&'static str; 42] = [
        "glide",
        "oscillator_2_frequency",
        "oscillator_3_frequency",
        "external_input_volume",
        "oscillator_1_volume",
        "oscillator_2_volume",
        "oscillator_3_volume",
        "noise_volume",
        "cutoff_frequency",
        "emphasis",
        "filter_attack_time",
        "filter_decay_time",
        "filter_sustain_level",
        "amount_of_contour",
        "loudness_attack_time",
        "loudness_decay_time",
        "loudness_sustain_level",
        "glide_switch",
        "oscillator_1_range_octave",
        "oscillator_2_range_octave",
        "oscillator_3_range_octave",
        "oscillator_3_control",
        "oscillator_3_range_lo_hi",
        "external_input_switch",
        "oscillator_1_switch",
        "oscillator_2_switch",
        "oscillator_3_switch",
        "noise_switch",
        "decay_switch",
        "tune",
        "modulation_mix",
        "oscillator_1_range_lo_hi",
        "oscillator_1_waveform",
        "oscillator_2_range_lo_hi",
        "oscillator_2_waveform",
        "oscillator_3_waveform",
        "main_output_volume",
        "oscillator_modulation",
        "noise_colour",
        "filter_modulation",
        "keyboard_control_1",
        "keyboard_control_2",
    ];

    /// The parameter's name.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one arm for each of the family's named parameters")]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::glide(_) => "glide",
            Self::oscillator_2_frequency(_) => "oscillator_2_frequency",
            Self::oscillator_3_frequency(_) => "oscillator_3_frequency",
            Self::external_input_volume(_) => "external_input_volume",
            Self::oscillator_1_volume(_) => "oscillator_1_volume",
            Self::oscillator_2_volume(_) => "oscillator_2_volume",
            Self::oscillator_3_volume(_) => "oscillator_3_volume",
            Self::noise_volume(_) => "noise_volume",
            Self::cutoff_frequency(_) => "cutoff_frequency",
            Self::emphasis(_) => "emphasis",
            Self::filter_attack_time(_) => "filter_attack_time",
            Self::filter_decay_time(_) => "filter_decay_time",
            Self::filter_sustain_level(_) => "filter_sustain_level",
            Self::amount_of_contour(_) => "amount_of_contour",
            Self::loudness_attack_time(_) => "loudness_attack_time",
            Self::loudness_decay_time(_) => "loudness_decay_time",
            Self::loudness_sustain_level(_) => "loudness_sustain_level",
            Self::glide_switch(_) => "glide_switch",
            Self::oscillator_1_range_octave(_) => "oscillator_1_range_octave",
            Self::oscillator_2_range_octave(_) => "oscillator_2_range_octave",
            Self::oscillator_3_range_octave(_) => "oscillator_3_range_octave",
            Self::oscillator_3_control(_) => "oscillator_3_control",
            Self::oscillator_3_range_lo_hi(_) => "oscillator_3_range_lo_hi",
            Self::external_input_switch(_) => "external_input_switch",
            Self::oscillator_1_switch(_) => "oscillator_1_switch",
            Self::oscillator_2_switch(_) => "oscillator_2_switch",
            Self::oscillator_3_switch(_) => "oscillator_3_switch",
            Self::noise_switch(_) => "noise_switch",
            Self::decay_switch(_) => "decay_switch",
            Self::tune(_) => "tune",
            Self::modulation_mix(_) => "modulation_mix",
            Self::oscillator_1_range_lo_hi(_) => "oscillator_1_range_lo_hi",
            Self::oscillator_1_waveform(_) => "oscillator_1_waveform",
            Self::oscillator_2_range_lo_hi(_) => "oscillator_2_range_lo_hi",
            Self::oscillator_2_waveform(_) => "oscillator_2_waveform",
            Self::oscillator_3_waveform(_) => "oscillator_3_waveform",
            Self::main_output_volume(_) => "main_output_volume",
            Self::oscillator_modulation(_) => "oscillator_modulation",
            Self::noise_colour(_) => "noise_colour",
            Self::filter_modulation(_) => "filter_modulation",
            Self::keyboard_control_1(_) => "keyboard_control_1",
            Self::keyboard_control_2(_) => "keyboard_control_2",
        }
    }
}

/// A synth53, synth52 or synth51 program as named fields: the 66 program
/// parameters but parameter 20, FILTER REV SELECT, which the server writes
/// from the kind (synth53-synth52-synth51.md, S53-27, S53-38, S53-47;
/// server.md SRV-37).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth53Program {
    /// Program parameter 0, 0 to 120.
    pub osc_a_frequency: U8In<0, 120>,
    /// Program parameter 1, 0 to 120.
    pub osc_b_frequency: U8In<0, 120>,
    /// Program parameter 2, 0 to 127.
    pub osc_b_fine_tune: U8In<0, 127>,
    /// Program parameter 3, 0 to 1.
    pub osc_a_saw: U8In<0, 1>,
    /// Program parameter 4, 0 to 1.
    pub osc_a_square: U8In<0, 1>,
    /// Program parameter 5, 0 to 1.
    pub osc_b_saw: U8In<0, 1>,
    /// Program parameter 6, 0 to 1.
    pub osc_b_tri: U8In<0, 1>,
    /// Program parameter 7, 0 to 1.
    pub osc_b_square: U8In<0, 1>,
    /// Program parameter 8, 0 to 120.
    pub osc_a_pulse_width: U8In<0, 120>,
    /// Program parameter 9, 0 to 120.
    pub osc_b_pulse_width: U8In<0, 120>,
    /// Program parameter 10, 0 to 1.
    pub osc_sync: U8In<0, 1>,
    /// Program parameter 11, 0 to 1.
    pub osc_b_low_freq: U8In<0, 1>,
    /// Program parameter 12, 0 to 1.
    pub osc_b_keyboard: U8In<0, 1>,
    /// Program parameter 13, 0 to 120.
    pub glide_rate: U8In<0, 120>,
    /// Program parameter 14, 0 to 120.
    pub osc_a_level: U8In<0, 120>,
    /// Program parameter 15, 0 to 120.
    pub osc_b_level: U8In<0, 120>,
    /// Program parameter 16, 0 to 120.
    pub noise_level: U8In<0, 120>,
    /// Program parameter 17, 0 to 120.
    pub cutoff: U8In<0, 120>,
    /// Program parameter 18, 0 to 120.
    pub resonance: U8In<0, 120>,
    /// Program parameter 19, 0 to 2.
    pub filter_keyboard_track: U8In<0, 2>,
    /// Program parameter 21, 0 to 120.
    pub lfo_frequency: U8In<0, 120>,
    /// Program parameter 22, 0 to 120.
    pub lfo_initial_amount: U8In<0, 120>,
    /// Program parameter 23, 0 to 1.
    pub lfo_saw: U8In<0, 1>,
    /// Program parameter 24, 0 to 1.
    pub lfo_tri: U8In<0, 1>,
    /// Program parameter 25, 0 to 1.
    pub lfo_square: U8In<0, 1>,
    /// Program parameter 26, 0 to 120.
    pub lfo_source_mix: U8In<0, 120>,
    /// Program parameter 27, 0 to 1.
    pub lfo_freq_a: U8In<0, 1>,
    /// Program parameter 28, 0 to 1.
    pub lfo_freq_b: U8In<0, 1>,
    /// Program parameter 29, 0 to 1.
    pub lfo_freq_pw_a: U8In<0, 1>,
    /// Program parameter 30, 0 to 1.
    pub lfo_freq_pw_b: U8In<0, 1>,
    /// Program parameter 31, 0 to 1.
    pub lfo_filter: U8In<0, 1>,
    /// Program parameter 32, 0 to 127.
    pub poly_mod_filt_env_amount: U8In<0, 127>,
    /// Program parameter 33, 0 to 120.
    pub poly_mod_osc_b_amount: U8In<0, 120>,
    /// Program parameter 34, 0 to 1.
    pub poly_mod_freq_a: U8In<0, 1>,
    /// Program parameter 35, 0 to 1.
    pub poly_mod_pw: U8In<0, 1>,
    /// Program parameter 36, 0 to 1.
    pub poly_mod_filter: U8In<0, 1>,
    /// Program parameter 37, 0 to 127.
    pub vintage: U8In<0, 127>,
    /// Program parameter 38, 0 to 1.
    pub aftertouch_to_filter: U8In<0, 1>,
    /// Program parameter 39, 0 to 1.
    pub aftertouch_to_lfo: U8In<0, 1>,
    /// Program parameter 40, 0 to 120.
    pub env_filter_amount: U8In<0, 120>,
    /// Program parameter 41, 0 to 1.
    pub velocity_to_filter: U8In<0, 1>,
    /// Program parameter 42, 0 to 1.
    pub velocity_to_amp: U8In<0, 1>,
    /// Program parameter 43, 0 to 120.
    pub attack_filter: U8In<0, 120>,
    /// Program parameter 44, 0 to 120.
    pub attack_vca: U8In<0, 120>,
    /// Program parameter 45, 0 to 120.
    pub decay_filter: U8In<0, 120>,
    /// Program parameter 46, 0 to 120.
    pub decay_vca: U8In<0, 120>,
    /// Program parameter 47, 0 to 120.
    pub sustain_filter: U8In<0, 120>,
    /// Program parameter 48, 0 to 120.
    pub sustain_vca: U8In<0, 120>,
    /// Program parameter 49, 0 to 120.
    pub release_filter: U8In<0, 120>,
    /// Program parameter 50, 0 to 120.
    pub release_vca: U8In<0, 120>,
    /// Program parameter 51, 0 to 1.
    pub release_switch: U8In<0, 1>,
    /// Program parameter 52, 0 to 1.
    pub unison: U8In<0, 1>,
    /// Program parameter 53, 0 to 10.
    pub unison_voice_count: U8In<0, 10>,
    /// Program parameter 54, 0 to 7.
    pub unison_detune: U8In<0, 7>,
    /// UNISON NOTE 1 to 10, each 0 to 10: program parameters 55 to 64, in order.
    pub unison_notes: [U8In<0, 10>; 10],
    /// Program parameter 86, 0 to 11.
    pub pitch_wheel_range: U8In<0, 11>,
    /// Program parameter 87, 0 to 3.
    pub retrigger_and_unison: U8In<0, 3>,
}

/// One of the synth53 family's 66 named parameters, every program parameter
/// but parameter 20 (server.md SRV-39).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(non_camel_case_types, reason = "each variant is the parameter's name on the wire, which the schema and the kits read as written (server.md SRV-22)")]
pub enum Synth53Parameter {
    /// Program parameter 0, 0 to 120, as its NRPN.
    osc_a_frequency(U8In<0, 120>),
    /// Program parameter 1, 0 to 120, as its NRPN.
    osc_b_frequency(U8In<0, 120>),
    /// Program parameter 2, 0 to 127, as its NRPN.
    osc_b_fine_tune(U8In<0, 127>),
    /// Program parameter 3, 0 to 1, as its NRPN.
    osc_a_saw(U8In<0, 1>),
    /// Program parameter 4, 0 to 1, as its NRPN.
    osc_a_square(U8In<0, 1>),
    /// Program parameter 5, 0 to 1, as its NRPN.
    osc_b_saw(U8In<0, 1>),
    /// Program parameter 6, 0 to 1, as its NRPN.
    osc_b_tri(U8In<0, 1>),
    /// Program parameter 7, 0 to 1, as its NRPN.
    osc_b_square(U8In<0, 1>),
    /// Program parameter 8, 0 to 120, as its NRPN.
    osc_a_pulse_width(U8In<0, 120>),
    /// Program parameter 9, 0 to 120, as its NRPN.
    osc_b_pulse_width(U8In<0, 120>),
    /// Program parameter 10, 0 to 1, as its NRPN.
    osc_sync(U8In<0, 1>),
    /// Program parameter 11, 0 to 1, as its NRPN.
    osc_b_low_freq(U8In<0, 1>),
    /// Program parameter 12, 0 to 1, as its NRPN.
    osc_b_keyboard(U8In<0, 1>),
    /// Program parameter 13, 0 to 120, as its NRPN.
    glide_rate(U8In<0, 120>),
    /// Program parameter 14, 0 to 120, as its NRPN.
    osc_a_level(U8In<0, 120>),
    /// Program parameter 15, 0 to 120, as its NRPN.
    osc_b_level(U8In<0, 120>),
    /// Program parameter 16, 0 to 120, as its NRPN.
    noise_level(U8In<0, 120>),
    /// Program parameter 17, 0 to 120, as its NRPN.
    cutoff(U8In<0, 120>),
    /// Program parameter 18, 0 to 120, as its NRPN.
    resonance(U8In<0, 120>),
    /// Program parameter 19, 0 to 2, as its NRPN.
    filter_keyboard_track(U8In<0, 2>),
    /// Program parameter 21, 0 to 120, as its NRPN.
    lfo_frequency(U8In<0, 120>),
    /// Program parameter 22, 0 to 120, as its NRPN.
    lfo_initial_amount(U8In<0, 120>),
    /// Program parameter 23, 0 to 1, as its NRPN.
    lfo_saw(U8In<0, 1>),
    /// Program parameter 24, 0 to 1, as its NRPN.
    lfo_tri(U8In<0, 1>),
    /// Program parameter 25, 0 to 1, as its NRPN.
    lfo_square(U8In<0, 1>),
    /// Program parameter 26, 0 to 120, as its NRPN.
    lfo_source_mix(U8In<0, 120>),
    /// Program parameter 27, 0 to 1, as its NRPN.
    lfo_freq_a(U8In<0, 1>),
    /// Program parameter 28, 0 to 1, as its NRPN.
    lfo_freq_b(U8In<0, 1>),
    /// Program parameter 29, 0 to 1, as its NRPN.
    lfo_freq_pw_a(U8In<0, 1>),
    /// Program parameter 30, 0 to 1, as its NRPN.
    lfo_freq_pw_b(U8In<0, 1>),
    /// Program parameter 31, 0 to 1, as its NRPN.
    lfo_filter(U8In<0, 1>),
    /// Program parameter 32, 0 to 127, as its NRPN.
    poly_mod_filt_env_amount(U8In<0, 127>),
    /// Program parameter 33, 0 to 120, as its NRPN.
    poly_mod_osc_b_amount(U8In<0, 120>),
    /// Program parameter 34, 0 to 1, as its NRPN.
    poly_mod_freq_a(U8In<0, 1>),
    /// Program parameter 35, 0 to 1, as its NRPN.
    poly_mod_pw(U8In<0, 1>),
    /// Program parameter 36, 0 to 1, as its NRPN.
    poly_mod_filter(U8In<0, 1>),
    /// Program parameter 37, 0 to 127, as its NRPN.
    vintage(U8In<0, 127>),
    /// Program parameter 38, 0 to 1, as its NRPN.
    aftertouch_to_filter(U8In<0, 1>),
    /// Program parameter 39, 0 to 1, as its NRPN.
    aftertouch_to_lfo(U8In<0, 1>),
    /// Program parameter 40, 0 to 120, as its NRPN.
    env_filter_amount(U8In<0, 120>),
    /// Program parameter 41, 0 to 1, as its NRPN.
    velocity_to_filter(U8In<0, 1>),
    /// Program parameter 42, 0 to 1, as its NRPN.
    velocity_to_amp(U8In<0, 1>),
    /// Program parameter 43, 0 to 120, as its NRPN.
    attack_filter(U8In<0, 120>),
    /// Program parameter 44, 0 to 120, as its NRPN.
    attack_vca(U8In<0, 120>),
    /// Program parameter 45, 0 to 120, as its NRPN.
    decay_filter(U8In<0, 120>),
    /// Program parameter 46, 0 to 120, as its NRPN.
    decay_vca(U8In<0, 120>),
    /// Program parameter 47, 0 to 120, as its NRPN.
    sustain_filter(U8In<0, 120>),
    /// Program parameter 48, 0 to 120, as its NRPN.
    sustain_vca(U8In<0, 120>),
    /// Program parameter 49, 0 to 120, as its NRPN.
    release_filter(U8In<0, 120>),
    /// Program parameter 50, 0 to 120, as its NRPN.
    release_vca(U8In<0, 120>),
    /// Program parameter 51, 0 to 1, as its NRPN.
    release_switch(U8In<0, 1>),
    /// Program parameter 52, 0 to 1, as its NRPN.
    unison(U8In<0, 1>),
    /// Program parameter 53, 0 to 10, as its NRPN.
    unison_voice_count(U8In<0, 10>),
    /// Program parameter 54, 0 to 7, as its NRPN.
    unison_detune(U8In<0, 7>),
    /// UNISON NOTE 1, 0 to 10: program parameter 55, as its NRPN.
    unison_note_1(U8In<0, 10>),
    /// UNISON NOTE 2, 0 to 10: program parameter 56, as its NRPN.
    unison_note_2(U8In<0, 10>),
    /// UNISON NOTE 3, 0 to 10: program parameter 57, as its NRPN.
    unison_note_3(U8In<0, 10>),
    /// UNISON NOTE 4, 0 to 10: program parameter 58, as its NRPN.
    unison_note_4(U8In<0, 10>),
    /// UNISON NOTE 5, 0 to 10: program parameter 59, as its NRPN.
    unison_note_5(U8In<0, 10>),
    /// UNISON NOTE 6, 0 to 10: program parameter 60, as its NRPN.
    unison_note_6(U8In<0, 10>),
    /// UNISON NOTE 7, 0 to 10: program parameter 61, as its NRPN.
    unison_note_7(U8In<0, 10>),
    /// UNISON NOTE 8, 0 to 10: program parameter 62, as its NRPN.
    unison_note_8(U8In<0, 10>),
    /// UNISON NOTE 9, 0 to 10: program parameter 63, as its NRPN.
    unison_note_9(U8In<0, 10>),
    /// UNISON NOTE 10, 0 to 10: program parameter 64, as its NRPN.
    unison_note_10(U8In<0, 10>),
    /// Program parameter 86, 0 to 11, as its NRPN.
    pitch_wheel_range(U8In<0, 11>),
    /// Program parameter 87, 0 to 3, as its NRPN.
    retrigger_and_unison(U8In<0, 3>),
}

impl Synth53Parameter {
    /// Every parameter's name, in the order of the family document's table.
    pub const NAMES: [&'static str; 66] = [
        "osc_a_frequency",
        "osc_b_frequency",
        "osc_b_fine_tune",
        "osc_a_saw",
        "osc_a_square",
        "osc_b_saw",
        "osc_b_tri",
        "osc_b_square",
        "osc_a_pulse_width",
        "osc_b_pulse_width",
        "osc_sync",
        "osc_b_low_freq",
        "osc_b_keyboard",
        "glide_rate",
        "osc_a_level",
        "osc_b_level",
        "noise_level",
        "cutoff",
        "resonance",
        "filter_keyboard_track",
        "lfo_frequency",
        "lfo_initial_amount",
        "lfo_saw",
        "lfo_tri",
        "lfo_square",
        "lfo_source_mix",
        "lfo_freq_a",
        "lfo_freq_b",
        "lfo_freq_pw_a",
        "lfo_freq_pw_b",
        "lfo_filter",
        "poly_mod_filt_env_amount",
        "poly_mod_osc_b_amount",
        "poly_mod_freq_a",
        "poly_mod_pw",
        "poly_mod_filter",
        "vintage",
        "aftertouch_to_filter",
        "aftertouch_to_lfo",
        "env_filter_amount",
        "velocity_to_filter",
        "velocity_to_amp",
        "attack_filter",
        "attack_vca",
        "decay_filter",
        "decay_vca",
        "sustain_filter",
        "sustain_vca",
        "release_filter",
        "release_vca",
        "release_switch",
        "unison",
        "unison_voice_count",
        "unison_detune",
        "unison_note_1",
        "unison_note_2",
        "unison_note_3",
        "unison_note_4",
        "unison_note_5",
        "unison_note_6",
        "unison_note_7",
        "unison_note_8",
        "unison_note_9",
        "unison_note_10",
        "pitch_wheel_range",
        "retrigger_and_unison",
    ];

    /// The parameter's name.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one arm for each of the family's named parameters")]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::osc_a_frequency(_) => "osc_a_frequency",
            Self::osc_b_frequency(_) => "osc_b_frequency",
            Self::osc_b_fine_tune(_) => "osc_b_fine_tune",
            Self::osc_a_saw(_) => "osc_a_saw",
            Self::osc_a_square(_) => "osc_a_square",
            Self::osc_b_saw(_) => "osc_b_saw",
            Self::osc_b_tri(_) => "osc_b_tri",
            Self::osc_b_square(_) => "osc_b_square",
            Self::osc_a_pulse_width(_) => "osc_a_pulse_width",
            Self::osc_b_pulse_width(_) => "osc_b_pulse_width",
            Self::osc_sync(_) => "osc_sync",
            Self::osc_b_low_freq(_) => "osc_b_low_freq",
            Self::osc_b_keyboard(_) => "osc_b_keyboard",
            Self::glide_rate(_) => "glide_rate",
            Self::osc_a_level(_) => "osc_a_level",
            Self::osc_b_level(_) => "osc_b_level",
            Self::noise_level(_) => "noise_level",
            Self::cutoff(_) => "cutoff",
            Self::resonance(_) => "resonance",
            Self::filter_keyboard_track(_) => "filter_keyboard_track",
            Self::lfo_frequency(_) => "lfo_frequency",
            Self::lfo_initial_amount(_) => "lfo_initial_amount",
            Self::lfo_saw(_) => "lfo_saw",
            Self::lfo_tri(_) => "lfo_tri",
            Self::lfo_square(_) => "lfo_square",
            Self::lfo_source_mix(_) => "lfo_source_mix",
            Self::lfo_freq_a(_) => "lfo_freq_a",
            Self::lfo_freq_b(_) => "lfo_freq_b",
            Self::lfo_freq_pw_a(_) => "lfo_freq_pw_a",
            Self::lfo_freq_pw_b(_) => "lfo_freq_pw_b",
            Self::lfo_filter(_) => "lfo_filter",
            Self::poly_mod_filt_env_amount(_) => "poly_mod_filt_env_amount",
            Self::poly_mod_osc_b_amount(_) => "poly_mod_osc_b_amount",
            Self::poly_mod_freq_a(_) => "poly_mod_freq_a",
            Self::poly_mod_pw(_) => "poly_mod_pw",
            Self::poly_mod_filter(_) => "poly_mod_filter",
            Self::vintage(_) => "vintage",
            Self::aftertouch_to_filter(_) => "aftertouch_to_filter",
            Self::aftertouch_to_lfo(_) => "aftertouch_to_lfo",
            Self::env_filter_amount(_) => "env_filter_amount",
            Self::velocity_to_filter(_) => "velocity_to_filter",
            Self::velocity_to_amp(_) => "velocity_to_amp",
            Self::attack_filter(_) => "attack_filter",
            Self::attack_vca(_) => "attack_vca",
            Self::decay_filter(_) => "decay_filter",
            Self::decay_vca(_) => "decay_vca",
            Self::sustain_filter(_) => "sustain_filter",
            Self::sustain_vca(_) => "sustain_vca",
            Self::release_filter(_) => "release_filter",
            Self::release_vca(_) => "release_vca",
            Self::release_switch(_) => "release_switch",
            Self::unison(_) => "unison",
            Self::unison_voice_count(_) => "unison_voice_count",
            Self::unison_detune(_) => "unison_detune",
            Self::unison_note_1(_) => "unison_note_1",
            Self::unison_note_2(_) => "unison_note_2",
            Self::unison_note_3(_) => "unison_note_3",
            Self::unison_note_4(_) => "unison_note_4",
            Self::unison_note_5(_) => "unison_note_5",
            Self::unison_note_6(_) => "unison_note_6",
            Self::unison_note_7(_) => "unison_note_7",
            Self::unison_note_8(_) => "unison_note_8",
            Self::unison_note_9(_) => "unison_note_9",
            Self::unison_note_10(_) => "unison_note_10",
            Self::pitch_wheel_range(_) => "pitch_wheel_range",
            Self::retrigger_and_unison(_) => "retrigger_and_unison",
        }
    }
}

/// A synth8 patch as named fields: the value of each of the 42 controllers of
/// synth8.md, S8-23, in byte order (server.md SRV-40).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth8Panel {
    /// LFO RATE: controller 29, 0 to 127 (byte 0 of S8-23).
    pub lfo_rate: U8In<0, 127>,
    /// LFO DELAY TIME: controller 27, 0 to 127 (byte 1 of S8-23).
    pub lfo_delay_time: U8In<0, 127>,
    /// LFO WAVE FORM: controller 35, 0 to 127 (byte 2 of S8-23).
    pub lfo_waveform: U8In<0, 127>,
    /// VCO LFO MOD: controller 26, 0 to 127 (byte 3 of S8-23).
    pub vco_lfo_mod: U8In<0, 127>,
    /// VCO ENV MOD: controller 25, 0 to 127 (byte 4 of S8-23).
    pub vco_env_mod: U8In<0, 127>,
    /// VCO MODULATOR switch: controller 60, 0 to 127 (byte 5 of S8-23).
    pub vco_modulator: U8In<0, 127>,
    /// PULSE WIDTH MOD: controller 18, 0 to 127 (byte 6 of S8-23).
    pub pulse_width_mod: U8In<0, 127>,
    /// PULSE WIDTH MOD source: controller 19, 0 to 127 (byte 7 of S8-23).
    pub pulse_width_mod_source: U8In<0, 127>,
    /// CROSS MOD: controller 59, 0 to 127 (byte 8 of S8-23).
    pub cross_mod: U8In<0, 127>,
    /// VCO-1 RANGE: controller 20, 0 to 127 (byte 9 of S8-23).
    pub vco1_range: U8In<0, 127>,
    /// VCO-1 WAVEFORM: controller 46, 0 to 127 (byte 10 of S8-23).
    pub vco1_waveform: U8In<0, 127>,
    /// VCO-2 SYNC: controller 87, 0 to 127 (byte 11 of S8-23).
    pub vco2_sync: U8In<0, 127>,
    /// VCO-2 NORMAL / LOW FREQ switch: controller 57, 0 to 127 (byte 12 of S8-23).
    pub vco2_low_freq: U8In<0, 127>,
    /// VCO-2 RANGE: controller 62, 0 to 127 (byte 13 of S8-23).
    pub vco2_range: U8In<0, 127>,
    /// VCO-2 FINE TUNE: controller 56, 0 to 127 (byte 14 of S8-23).
    pub vco2_fine_tune: U8In<0, 127>,
    /// VCO-2 WAVEFORM: controller 61, 0 to 127 (byte 15 of S8-23).
    pub vco2_waveform: U8In<0, 127>,
    /// VCO-1 LEVEL: controller 16, 0 to 127 (byte 16 of S8-23).
    pub vco1_level: U8In<0, 127>,
    /// VCO-2 LEVEL: controller 17, 0 to 127 (byte 17 of S8-23).
    pub vco2_level: U8In<0, 127>,
    /// HPF CUTOFF: controller 79, 0 to 127 (byte 18 of S8-23).
    pub hpf_cutoff: U8In<0, 127>,
    /// VCF CUTOFF: controller 3, 0 to 127 (byte 19 of S8-23).
    pub vcf_cutoff: U8In<0, 127>,
    /// VCF RESONANCE: controller 9, 0 to 127 (byte 20 of S8-23).
    pub vcf_resonance: U8In<0, 127>,
    /// VCF SLOPE: controller 13, 0 to 127 (byte 21 of S8-23).
    pub vcf_slope: U8In<0, 127>,
    /// VCF ENV MOD: controller 81, 0 to 127 (byte 22 of S8-23).
    pub vcf_env_mod: U8In<0, 127>,
    /// VCF ENVELOPE SEL: controller 84, 0 to 127 (byte 23 of S8-23).
    pub vcf_envelope_select: U8In<0, 127>,
    /// VCF LFO MOD: controller 28, 0 to 127 (byte 24 of S8-23).
    pub vcf_lfo_mod: U8In<0, 127>,
    /// VCF KEY FOLLOW: controller 82, 0 to 127 (byte 25 of S8-23).
    pub vcf_key_follow: U8In<0, 127>,
    /// VCA LEVEL: controller 110, 0 to 127 (byte 26 of S8-23).
    pub vca_level: U8In<0, 127>,
    /// VCA LFO MOD: controller 109, 0 to 127 (byte 27 of S8-23).
    pub vca_lfo_mod: U8In<0, 127>,
    /// ENV-1 ATTACK: controller 83, 0 to 127 (byte 28 of S8-23).
    pub env1_attack: U8In<0, 127>,
    /// ENV-1 DECAY: controller 80, 0 to 127 (byte 29 of S8-23).
    pub env1_decay: U8In<0, 127>,
    /// ENV-1 SUSTAIN: controller 85, 0 to 127 (byte 30 of S8-23).
    pub env1_sustain: U8In<0, 127>,
    /// ENV-1 RELEASE: controller 86, 0 to 127 (byte 31 of S8-23).
    pub env1_release: U8In<0, 127>,
    /// ENV-1 POLARITY: controller 106, 0 to 127 (byte 32 of S8-23).
    pub env1_polarity: U8In<0, 127>,
    /// ENV-2 ATTACK: controller 89, 0 to 127 (byte 33 of S8-23).
    pub env2_attack: U8In<0, 127>,
    /// ENV-2 DECAY: controller 90, 0 to 127 (byte 34 of S8-23).
    pub env2_decay: U8In<0, 127>,
    /// ENV-2 SUSTAIN: controller 102, 0 to 127 (byte 35 of S8-23).
    pub env2_sustain: U8In<0, 127>,
    /// ENV-2 RELEASE: controller 103, 0 to 127 (byte 36 of S8-23).
    pub env2_release: U8In<0, 127>,
    /// ASSIGN MODE: controller 119, 0 to 127 (byte 37 of S8-23).
    pub assign_mode: U8In<0, 127>,
    /// ENV-1 KEY FOLLOW: controller 104, 0 to 127 (byte 38 of S8-23).
    pub env1_key_follow: U8In<0, 127>,
    /// ENV-2 KEY FOLLOW: controller 105, 0 to 127 (byte 39 of S8-23).
    pub env2_key_follow: U8In<0, 127>,
    /// VCO-1 BEND, NeoTeletext's own: controller 107, 0 to 127 (byte 40 of S8-23).
    pub vco1_bend: U8In<0, 127>,
    /// VCO-2 BEND, NeoTeletext's own: controller 108, 0 to 127 (byte 41 of S8-23).
    pub vco2_bend: U8In<0, 127>,
}

/// One of synth8's named parameters, one for each field of its panel, on
/// channel 1 (the upper half) or 2 (the lower) (server.md SRV-42).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(non_camel_case_types, reason = "each variant is the parameter's name on the wire, which the schema and the kits read as written (server.md SRV-22)")]
pub enum Synth8Parameter {
    /// LFO RATE: controller 29, 0 to 127 (byte 0 of S8-23).
    lfo_rate(U8In<0, 127>),
    /// LFO DELAY TIME: controller 27, 0 to 127 (byte 1 of S8-23).
    lfo_delay_time(U8In<0, 127>),
    /// LFO WAVE FORM: controller 35, 0 to 127 (byte 2 of S8-23).
    lfo_waveform(U8In<0, 127>),
    /// VCO LFO MOD: controller 26, 0 to 127 (byte 3 of S8-23).
    vco_lfo_mod(U8In<0, 127>),
    /// VCO ENV MOD: controller 25, 0 to 127 (byte 4 of S8-23).
    vco_env_mod(U8In<0, 127>),
    /// VCO MODULATOR switch: controller 60, 0 to 127 (byte 5 of S8-23).
    vco_modulator(U8In<0, 127>),
    /// PULSE WIDTH MOD: controller 18, 0 to 127 (byte 6 of S8-23).
    pulse_width_mod(U8In<0, 127>),
    /// PULSE WIDTH MOD source: controller 19, 0 to 127 (byte 7 of S8-23).
    pulse_width_mod_source(U8In<0, 127>),
    /// CROSS MOD: controller 59, 0 to 127 (byte 8 of S8-23).
    cross_mod(U8In<0, 127>),
    /// VCO-1 RANGE: controller 20, 0 to 127 (byte 9 of S8-23).
    vco1_range(U8In<0, 127>),
    /// VCO-1 WAVEFORM: controller 46, 0 to 127 (byte 10 of S8-23).
    vco1_waveform(U8In<0, 127>),
    /// VCO-2 SYNC: controller 87, 0 to 127 (byte 11 of S8-23).
    vco2_sync(U8In<0, 127>),
    /// VCO-2 NORMAL / LOW FREQ switch: controller 57, 0 to 127 (byte 12 of S8-23).
    vco2_low_freq(U8In<0, 127>),
    /// VCO-2 RANGE: controller 62, 0 to 127 (byte 13 of S8-23).
    vco2_range(U8In<0, 127>),
    /// VCO-2 FINE TUNE: controller 56, 0 to 127 (byte 14 of S8-23).
    vco2_fine_tune(U8In<0, 127>),
    /// VCO-2 WAVEFORM: controller 61, 0 to 127 (byte 15 of S8-23).
    vco2_waveform(U8In<0, 127>),
    /// VCO-1 LEVEL: controller 16, 0 to 127 (byte 16 of S8-23).
    vco1_level(U8In<0, 127>),
    /// VCO-2 LEVEL: controller 17, 0 to 127 (byte 17 of S8-23).
    vco2_level(U8In<0, 127>),
    /// HPF CUTOFF: controller 79, 0 to 127 (byte 18 of S8-23).
    hpf_cutoff(U8In<0, 127>),
    /// VCF CUTOFF: controller 3, 0 to 127 (byte 19 of S8-23).
    vcf_cutoff(U8In<0, 127>),
    /// VCF RESONANCE: controller 9, 0 to 127 (byte 20 of S8-23).
    vcf_resonance(U8In<0, 127>),
    /// VCF SLOPE: controller 13, 0 to 127 (byte 21 of S8-23).
    vcf_slope(U8In<0, 127>),
    /// VCF ENV MOD: controller 81, 0 to 127 (byte 22 of S8-23).
    vcf_env_mod(U8In<0, 127>),
    /// VCF ENVELOPE SEL: controller 84, 0 to 127 (byte 23 of S8-23).
    vcf_envelope_select(U8In<0, 127>),
    /// VCF LFO MOD: controller 28, 0 to 127 (byte 24 of S8-23).
    vcf_lfo_mod(U8In<0, 127>),
    /// VCF KEY FOLLOW: controller 82, 0 to 127 (byte 25 of S8-23).
    vcf_key_follow(U8In<0, 127>),
    /// VCA LEVEL: controller 110, 0 to 127 (byte 26 of S8-23).
    vca_level(U8In<0, 127>),
    /// VCA LFO MOD: controller 109, 0 to 127 (byte 27 of S8-23).
    vca_lfo_mod(U8In<0, 127>),
    /// ENV-1 ATTACK: controller 83, 0 to 127 (byte 28 of S8-23).
    env1_attack(U8In<0, 127>),
    /// ENV-1 DECAY: controller 80, 0 to 127 (byte 29 of S8-23).
    env1_decay(U8In<0, 127>),
    /// ENV-1 SUSTAIN: controller 85, 0 to 127 (byte 30 of S8-23).
    env1_sustain(U8In<0, 127>),
    /// ENV-1 RELEASE: controller 86, 0 to 127 (byte 31 of S8-23).
    env1_release(U8In<0, 127>),
    /// ENV-1 POLARITY: controller 106, 0 to 127 (byte 32 of S8-23).
    env1_polarity(U8In<0, 127>),
    /// ENV-2 ATTACK: controller 89, 0 to 127 (byte 33 of S8-23).
    env2_attack(U8In<0, 127>),
    /// ENV-2 DECAY: controller 90, 0 to 127 (byte 34 of S8-23).
    env2_decay(U8In<0, 127>),
    /// ENV-2 SUSTAIN: controller 102, 0 to 127 (byte 35 of S8-23).
    env2_sustain(U8In<0, 127>),
    /// ENV-2 RELEASE: controller 103, 0 to 127 (byte 36 of S8-23).
    env2_release(U8In<0, 127>),
    /// ASSIGN MODE: controller 119, 0 to 127 (byte 37 of S8-23).
    assign_mode(U8In<0, 127>),
    /// ENV-1 KEY FOLLOW: controller 104, 0 to 127 (byte 38 of S8-23).
    env1_key_follow(U8In<0, 127>),
    /// ENV-2 KEY FOLLOW: controller 105, 0 to 127 (byte 39 of S8-23).
    env2_key_follow(U8In<0, 127>),
    /// VCO-1 BEND, NeoTeletext's own: controller 107, 0 to 127 (byte 40 of S8-23).
    vco1_bend(U8In<0, 127>),
    /// VCO-2 BEND, NeoTeletext's own: controller 108, 0 to 127 (byte 41 of S8-23).
    vco2_bend(U8In<0, 127>),
}

impl Synth8Parameter {
    /// Every parameter's name, in the order of the family document's table.
    pub const NAMES: [&'static str; 42] = [
        "lfo_rate",
        "lfo_delay_time",
        "lfo_waveform",
        "vco_lfo_mod",
        "vco_env_mod",
        "vco_modulator",
        "pulse_width_mod",
        "pulse_width_mod_source",
        "cross_mod",
        "vco1_range",
        "vco1_waveform",
        "vco2_sync",
        "vco2_low_freq",
        "vco2_range",
        "vco2_fine_tune",
        "vco2_waveform",
        "vco1_level",
        "vco2_level",
        "hpf_cutoff",
        "vcf_cutoff",
        "vcf_resonance",
        "vcf_slope",
        "vcf_env_mod",
        "vcf_envelope_select",
        "vcf_lfo_mod",
        "vcf_key_follow",
        "vca_level",
        "vca_lfo_mod",
        "env1_attack",
        "env1_decay",
        "env1_sustain",
        "env1_release",
        "env1_polarity",
        "env2_attack",
        "env2_decay",
        "env2_sustain",
        "env2_release",
        "assign_mode",
        "env1_key_follow",
        "env2_key_follow",
        "vco1_bend",
        "vco2_bend",
    ];

    /// The parameter's name.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one arm for each of the family's named parameters")]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::lfo_rate(_) => "lfo_rate",
            Self::lfo_delay_time(_) => "lfo_delay_time",
            Self::lfo_waveform(_) => "lfo_waveform",
            Self::vco_lfo_mod(_) => "vco_lfo_mod",
            Self::vco_env_mod(_) => "vco_env_mod",
            Self::vco_modulator(_) => "vco_modulator",
            Self::pulse_width_mod(_) => "pulse_width_mod",
            Self::pulse_width_mod_source(_) => "pulse_width_mod_source",
            Self::cross_mod(_) => "cross_mod",
            Self::vco1_range(_) => "vco1_range",
            Self::vco1_waveform(_) => "vco1_waveform",
            Self::vco2_sync(_) => "vco2_sync",
            Self::vco2_low_freq(_) => "vco2_low_freq",
            Self::vco2_range(_) => "vco2_range",
            Self::vco2_fine_tune(_) => "vco2_fine_tune",
            Self::vco2_waveform(_) => "vco2_waveform",
            Self::vco1_level(_) => "vco1_level",
            Self::vco2_level(_) => "vco2_level",
            Self::hpf_cutoff(_) => "hpf_cutoff",
            Self::vcf_cutoff(_) => "vcf_cutoff",
            Self::vcf_resonance(_) => "vcf_resonance",
            Self::vcf_slope(_) => "vcf_slope",
            Self::vcf_env_mod(_) => "vcf_env_mod",
            Self::vcf_envelope_select(_) => "vcf_envelope_select",
            Self::vcf_lfo_mod(_) => "vcf_lfo_mod",
            Self::vcf_key_follow(_) => "vcf_key_follow",
            Self::vca_level(_) => "vca_level",
            Self::vca_lfo_mod(_) => "vca_lfo_mod",
            Self::env1_attack(_) => "env1_attack",
            Self::env1_decay(_) => "env1_decay",
            Self::env1_sustain(_) => "env1_sustain",
            Self::env1_release(_) => "env1_release",
            Self::env1_polarity(_) => "env1_polarity",
            Self::env2_attack(_) => "env2_attack",
            Self::env2_decay(_) => "env2_decay",
            Self::env2_sustain(_) => "env2_sustain",
            Self::env2_release(_) => "env2_release",
            Self::assign_mode(_) => "assign_mode",
            Self::env1_key_follow(_) => "env1_key_follow",
            Self::env2_key_follow(_) => "env2_key_follow",
            Self::vco1_bend(_) => "vco1_bend",
            Self::vco2_bend(_) => "vco2_bend",
        }
    }
}

/// A synth80 channel's tone as its 26 named fields (synth80.md, S80-10,
/// S80-33; server.md SRV-43): bytes 0 to 25 for channel I, 26 to 51 for channel II.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth80ToneFields {
    /// PWM SPEED: controller 40 on channel I, 67 on channel II; 0 to 127.
    pub pwm_speed: U8In<0, 127>,
    /// PWM: controller 41 on channel I, 68 on channel II; 0 to 127.
    pub pwm: U8In<0, 127>,
    /// PW: controller 42 on channel I, 69 on channel II; 0 to 127.
    pub pw: U8In<0, 127>,
    /// Square wave switch: controller 43 on channel I, 70 on channel II; 0 to 127.
    pub square: U8In<0, 127>,
    /// Sawtooth wave switch: controller 44 on channel I, 71 on channel II; 0 to 127.
    pub sawtooth: U8In<0, 127>,
    /// NOISE: controller 45 on channel I, 72 on channel II; 0 to 127.
    pub noise: U8In<0, 127>,
    /// HPF: controller 46 on channel I, 73 on channel II; 0 to 127.
    pub hpf: U8In<0, 127>,
    /// `RESh`: controller 47 on channel I, 119 on channel II; 0 to 127.
    pub hpf_resonance: U8In<0, 127>,
    /// LPF: controller 48 on channel I, 75 on channel II; 0 to 127.
    pub lpf: U8In<0, 127>,
    /// `RESl`: controller 49 on channel I, 76 on channel II; 0 to 127.
    pub lpf_resonance: U8In<0, 127>,
    /// IL, the filter envelope's initial level: controller 50 on channel I, 77 on channel II; 0 to 127.
    pub filter_initial_level: U8In<0, 127>,
    /// AL, the filter envelope's attack level: controller 51 on channel I, 78 on channel II; 0 to 127.
    pub filter_attack_level: U8In<0, 127>,
    /// Filter envelope A: controller 52 on channel I, 79 on channel II; 0 to 127.
    pub filter_attack: U8In<0, 127>,
    /// Filter envelope D: controller 53 on channel I, 80 on channel II; 0 to 127.
    pub filter_decay: U8In<0, 127>,
    /// Filter envelope R: controller 54 on channel I, 81 on channel II; 0 to 127.
    pub filter_release: U8In<0, 127>,
    /// VCF LEVEL: controller 55 on channel I, 82 on channel II; 0 to 127.
    pub vcf_level: U8In<0, 127>,
    /// SINE: controller 56 on channel I, 83 on channel II; 0 to 127.
    pub sine: U8In<0, 127>,
    /// Amplifier envelope A: controller 57 on channel I, 84 on channel II; 0 to 127.
    pub amplifier_attack: U8In<0, 127>,
    /// Amplifier envelope D: controller 58 on channel I, 85 on channel II; 0 to 127.
    pub amplifier_decay: U8In<0, 127>,
    /// Amplifier envelope S: controller 59 on channel I, 86 on channel II; 0 to 127.
    pub amplifier_sustain: U8In<0, 127>,
    /// Amplifier envelope R: controller 60 on channel I, 87 on channel II; 0 to 127.
    pub amplifier_release: U8In<0, 127>,
    /// LEVEL: controller 61 on channel I, 88 on channel II; 0 to 127.
    pub level: U8In<0, 127>,
    /// INITIAL BRILLIANCE: controller 62 on channel I, 89 on channel II; 0 to 127.
    pub initial_brilliance: U8In<0, 127>,
    /// INITIAL LEVEL: controller 63 on channel I, 90 on channel II; 0 to 127.
    pub initial_level: U8In<0, 127>,
    /// AFTER BRILLIANCE: controller 65 on channel I, 91 on channel II; 0 to 127.
    pub after_brilliance: U8In<0, 127>,
    /// AFTER LEVEL: controller 66 on channel I, 92 on channel II; 0 to 127.
    pub after_level: U8In<0, 127>,
}

/// A synth80 patch as named fields: the two channels' tones and the 33 master
/// controls of synth80.md, S80-33 (server.md SRV-43).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth80Panel {
    /// Channel I's tone: its fields, or a preset tone.
    pub channel_1: Synth80Tone,
    /// Channel II's tone: its fields, or a preset tone.
    pub channel_2: Synth80Tone,
    /// FEET I: controller 102, 0 to 127 (byte 52 of S80-33).
    pub feet_1: U8In<0, 127>,
    /// FEET II: controller 103, 0 to 127 (byte 53 of S80-33).
    pub feet_2: U8In<0, 127>,
    /// DETUNE CH II: controller 95, 0 to 127 (byte 54 of S80-33).
    pub detune_channel_2: U8In<0, 127>,
    /// MIX I-II: controller 8, 0 to 127 (byte 55 of S80-33).
    pub mix: U8In<0, 127>,
    /// BRILLIANCE: controller 109, 0 to 127 (byte 56 of S80-33).
    pub brilliance: U8In<0, 127>,
    /// RESONANCE: controller 110, 0 to 127 (byte 57 of S80-33).
    pub resonance: U8In<0, 127>,
    /// KEYBOARD CONTROL BRILLIANCE LOW: controller 115, 0 to 127 (byte 58 of S80-33).
    pub keyboard_brilliance_low: U8In<0, 127>,
    /// KEYBOARD CONTROL BRILLIANCE HIGH: controller 116, 0 to 127 (byte 59 of S80-33).
    pub keyboard_brilliance_high: U8In<0, 127>,
    /// KEYBOARD CONTROL LEVEL LOW: controller 117, 0 to 127 (byte 60 of S80-33).
    pub keyboard_level_low: U8In<0, 127>,
    /// KEYBOARD CONTROL LEVEL HIGH: controller 118, 0 to 127 (byte 61 of S80-33).
    pub keyboard_level_high: U8In<0, 127>,
    /// SUB OSCILLATOR FUNCTION: controller 104, 0 to 127 (byte 62 of S80-33).
    pub sub_oscillator_function: U8In<0, 127>,
    /// SUB OSCILLATOR SPEED: controller 105, 0 to 127 (byte 63 of S80-33).
    pub sub_oscillator_speed: U8In<0, 127>,
    /// SUB OSCILLATOR VCO: controller 106, 0 to 127 (byte 64 of S80-33).
    pub sub_oscillator_vco: U8In<0, 127>,
    /// SUB OSCILLATOR VCF: controller 107, 0 to 127 (byte 65 of S80-33).
    pub sub_oscillator_vcf: U8In<0, 127>,
    /// SUB OSCILLATOR VCA: controller 108, 0 to 127 (byte 66 of S80-33).
    pub sub_oscillator_vca: U8In<0, 127>,
    /// TOUCH RESPONSE PITCHBEND: controller 111, 0 to 127 (byte 67 of S80-33).
    pub touch_pitch_bend: U8In<0, 127>,
    /// TOUCH RESPONSE SPEED: controller 112, 0 to 127 (byte 68 of S80-33).
    pub touch_speed: U8In<0, 127>,
    /// TOUCH RESPONSE VCO: controller 113, 0 to 127 (byte 69 of S80-33).
    pub touch_vco: U8In<0, 127>,
    /// TOUCH RESPONSE VCF: controller 114, 0 to 127 (byte 70 of S80-33).
    pub touch_vcf: U8In<0, 127>,
    /// RING MODULATOR ATTACK TIME, NeoTeletext's own: controller 20, 0 to 127 (byte 71 of S80-33).
    pub ring_modulator_attack_time: U8In<0, 127>,
    /// RING MODULATOR DECAY TIME, NeoTeletext's own: controller 21, 0 to 127 (byte 72 of S80-33).
    pub ring_modulator_decay_time: U8In<0, 127>,
    /// RING MODULATOR DEPTH, NeoTeletext's own: controller 22, 0 to 127 (byte 73 of S80-33).
    pub ring_modulator_depth: U8In<0, 127>,
    /// RING MODULATOR SPEED, NeoTeletext's own: controller 23, 0 to 127 (byte 74 of S80-33).
    pub ring_modulator_speed: U8In<0, 127>,
    /// RING MODULATOR MODULATION, NeoTeletext's own: controller 24, 0 to 127 (byte 75 of S80-33).
    pub ring_modulator_modulation: U8In<0, 127>,
    /// SUSTAIN I-II, NeoTeletext's own: controller 25, 0 to 127 (byte 76 of S80-33).
    pub sustain_mode: U8In<0, 127>,
    /// SUSTAIN, NeoTeletext's own: controller 26, 0 to 127 (byte 77 of S80-33).
    pub sustain: U8In<0, 127>,
    /// PORTAMENTO/GLISSANDO selector, NeoTeletext's own (S80-20): controller 3, 0 to 127 (byte 78 of S80-33).
    pub glide_mode: U8In<0, 127>,
    /// PORTAMENTO/GLISSANDO speed: controller 5, 0 to 127 (byte 79 of S80-33).
    pub glide_speed: U8In<0, 127>,
    /// TREMOLO/CHORUS ON/OFF, NeoTeletext's own: controller 27, 0 to 127 (byte 80 of S80-33).
    pub tremolo_chorus: U8In<0, 127>,
    /// CHORUS, NeoTeletext's own: controller 28, 0 to 127 (byte 81 of S80-33).
    pub chorus: U8In<0, 127>,
    /// TREMOLO, NeoTeletext's own: controller 29, 0 to 127 (byte 82 of S80-33).
    pub tremolo: U8In<0, 127>,
    /// TREMOLO/CHORUS SPEED, NeoTeletext's own: controller 30, 0 to 127 (byte 83 of S80-33).
    pub tremolo_chorus_speed: U8In<0, 127>,
    /// TREMOLO/CHORUS DEPTH, NeoTeletext's own: controller 31, 0 to 127 (byte 84 of S80-33).
    pub tremolo_chorus_depth: U8In<0, 127>,
}

/// One of synth80's 85 named parameters (server.md SRV-45).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(non_camel_case_types, reason = "each variant is the parameter's name on the wire, which the schema and the kits read as written (server.md SRV-22)")]
pub enum Synth80Parameter {
    /// FEET I: controller 102, 0 to 127.
    feet_1(U8In<0, 127>),
    /// FEET II: controller 103, 0 to 127.
    feet_2(U8In<0, 127>),
    /// DETUNE CH II: controller 95, 0 to 127.
    detune_channel_2(U8In<0, 127>),
    /// MIX I-II: controller 8, 0 to 127.
    mix(U8In<0, 127>),
    /// BRILLIANCE: controller 109, 0 to 127.
    brilliance(U8In<0, 127>),
    /// RESONANCE: controller 110, 0 to 127.
    resonance(U8In<0, 127>),
    /// KEYBOARD CONTROL BRILLIANCE LOW: controller 115, 0 to 127.
    keyboard_brilliance_low(U8In<0, 127>),
    /// KEYBOARD CONTROL BRILLIANCE HIGH: controller 116, 0 to 127.
    keyboard_brilliance_high(U8In<0, 127>),
    /// KEYBOARD CONTROL LEVEL LOW: controller 117, 0 to 127.
    keyboard_level_low(U8In<0, 127>),
    /// KEYBOARD CONTROL LEVEL HIGH: controller 118, 0 to 127.
    keyboard_level_high(U8In<0, 127>),
    /// SUB OSCILLATOR FUNCTION: controller 104, 0 to 127.
    sub_oscillator_function(U8In<0, 127>),
    /// SUB OSCILLATOR SPEED: controller 105, 0 to 127.
    sub_oscillator_speed(U8In<0, 127>),
    /// SUB OSCILLATOR VCO: controller 106, 0 to 127.
    sub_oscillator_vco(U8In<0, 127>),
    /// SUB OSCILLATOR VCF: controller 107, 0 to 127.
    sub_oscillator_vcf(U8In<0, 127>),
    /// SUB OSCILLATOR VCA: controller 108, 0 to 127.
    sub_oscillator_vca(U8In<0, 127>),
    /// TOUCH RESPONSE PITCHBEND: controller 111, 0 to 127.
    touch_pitch_bend(U8In<0, 127>),
    /// TOUCH RESPONSE SPEED: controller 112, 0 to 127.
    touch_speed(U8In<0, 127>),
    /// TOUCH RESPONSE VCO: controller 113, 0 to 127.
    touch_vco(U8In<0, 127>),
    /// TOUCH RESPONSE VCF: controller 114, 0 to 127.
    touch_vcf(U8In<0, 127>),
    /// RING MODULATOR ATTACK TIME, NeoTeletext's own: controller 20, 0 to 127.
    ring_modulator_attack_time(U8In<0, 127>),
    /// RING MODULATOR DECAY TIME, NeoTeletext's own: controller 21, 0 to 127.
    ring_modulator_decay_time(U8In<0, 127>),
    /// RING MODULATOR DEPTH, NeoTeletext's own: controller 22, 0 to 127.
    ring_modulator_depth(U8In<0, 127>),
    /// RING MODULATOR SPEED, NeoTeletext's own: controller 23, 0 to 127.
    ring_modulator_speed(U8In<0, 127>),
    /// RING MODULATOR MODULATION, NeoTeletext's own: controller 24, 0 to 127.
    ring_modulator_modulation(U8In<0, 127>),
    /// SUSTAIN I-II, NeoTeletext's own: controller 25, 0 to 127.
    sustain_mode(U8In<0, 127>),
    /// SUSTAIN, NeoTeletext's own: controller 26, 0 to 127.
    sustain(U8In<0, 127>),
    /// PORTAMENTO/GLISSANDO selector, NeoTeletext's own (S80-20): controller 3, 0 to 127.
    glide_mode(U8In<0, 127>),
    /// PORTAMENTO/GLISSANDO speed: controller 5, 0 to 127.
    glide_speed(U8In<0, 127>),
    /// TREMOLO/CHORUS ON/OFF, NeoTeletext's own: controller 27, 0 to 127.
    tremolo_chorus(U8In<0, 127>),
    /// CHORUS, NeoTeletext's own: controller 28, 0 to 127.
    chorus(U8In<0, 127>),
    /// TREMOLO, NeoTeletext's own: controller 29, 0 to 127.
    tremolo(U8In<0, 127>),
    /// TREMOLO/CHORUS SPEED, NeoTeletext's own: controller 30, 0 to 127.
    tremolo_chorus_speed(U8In<0, 127>),
    /// TREMOLO/CHORUS DEPTH, NeoTeletext's own: controller 31, 0 to 127.
    tremolo_chorus_depth(U8In<0, 127>),
    /// Channel I's PWM SPEED: controller 40, 0 to 127.
    channel_1_pwm_speed(U8In<0, 127>),
    /// Channel I's PWM: controller 41, 0 to 127.
    channel_1_pwm(U8In<0, 127>),
    /// Channel I's PW: controller 42, 0 to 127.
    channel_1_pw(U8In<0, 127>),
    /// Channel I's Square wave switch: controller 43, 0 to 127.
    channel_1_square(U8In<0, 127>),
    /// Channel I's Sawtooth wave switch: controller 44, 0 to 127.
    channel_1_sawtooth(U8In<0, 127>),
    /// Channel I's NOISE: controller 45, 0 to 127.
    channel_1_noise(U8In<0, 127>),
    /// Channel I's HPF: controller 46, 0 to 127.
    channel_1_hpf(U8In<0, 127>),
    /// Channel I's `RESh`: controller 47, 0 to 127.
    channel_1_hpf_resonance(U8In<0, 127>),
    /// Channel I's LPF: controller 48, 0 to 127.
    channel_1_lpf(U8In<0, 127>),
    /// Channel I's `RESl`: controller 49, 0 to 127.
    channel_1_lpf_resonance(U8In<0, 127>),
    /// Channel I's IL, the filter envelope's initial level: controller 50, 0 to 127.
    channel_1_filter_initial_level(U8In<0, 127>),
    /// Channel I's AL, the filter envelope's attack level: controller 51, 0 to 127.
    channel_1_filter_attack_level(U8In<0, 127>),
    /// Channel I's Filter envelope A: controller 52, 0 to 127.
    channel_1_filter_attack(U8In<0, 127>),
    /// Channel I's Filter envelope D: controller 53, 0 to 127.
    channel_1_filter_decay(U8In<0, 127>),
    /// Channel I's Filter envelope R: controller 54, 0 to 127.
    channel_1_filter_release(U8In<0, 127>),
    /// Channel I's VCF LEVEL: controller 55, 0 to 127.
    channel_1_vcf_level(U8In<0, 127>),
    /// Channel I's SINE: controller 56, 0 to 127.
    channel_1_sine(U8In<0, 127>),
    /// Channel I's Amplifier envelope A: controller 57, 0 to 127.
    channel_1_amplifier_attack(U8In<0, 127>),
    /// Channel I's Amplifier envelope D: controller 58, 0 to 127.
    channel_1_amplifier_decay(U8In<0, 127>),
    /// Channel I's Amplifier envelope S: controller 59, 0 to 127.
    channel_1_amplifier_sustain(U8In<0, 127>),
    /// Channel I's Amplifier envelope R: controller 60, 0 to 127.
    channel_1_amplifier_release(U8In<0, 127>),
    /// Channel I's LEVEL: controller 61, 0 to 127.
    channel_1_level(U8In<0, 127>),
    /// Channel I's INITIAL BRILLIANCE: controller 62, 0 to 127.
    channel_1_initial_brilliance(U8In<0, 127>),
    /// Channel I's INITIAL LEVEL: controller 63, 0 to 127.
    channel_1_initial_level(U8In<0, 127>),
    /// Channel I's AFTER BRILLIANCE: controller 65, 0 to 127.
    channel_1_after_brilliance(U8In<0, 127>),
    /// Channel I's AFTER LEVEL: controller 66, 0 to 127.
    channel_1_after_level(U8In<0, 127>),
    /// Channel II's PWM SPEED: controller 67, 0 to 127.
    channel_2_pwm_speed(U8In<0, 127>),
    /// Channel II's PWM: controller 68, 0 to 127.
    channel_2_pwm(U8In<0, 127>),
    /// Channel II's PW: controller 69, 0 to 127.
    channel_2_pw(U8In<0, 127>),
    /// Channel II's Square wave switch: controller 70, 0 to 127.
    channel_2_square(U8In<0, 127>),
    /// Channel II's Sawtooth wave switch: controller 71, 0 to 127.
    channel_2_sawtooth(U8In<0, 127>),
    /// Channel II's NOISE: controller 72, 0 to 127.
    channel_2_noise(U8In<0, 127>),
    /// Channel II's HPF: controller 73, 0 to 127.
    channel_2_hpf(U8In<0, 127>),
    /// Channel II's `RESh`: controller 119, 0 to 127.
    channel_2_hpf_resonance(U8In<0, 127>),
    /// Channel II's LPF: controller 75, 0 to 127.
    channel_2_lpf(U8In<0, 127>),
    /// Channel II's `RESl`: controller 76, 0 to 127.
    channel_2_lpf_resonance(U8In<0, 127>),
    /// Channel II's IL, the filter envelope's initial level: controller 77, 0 to 127.
    channel_2_filter_initial_level(U8In<0, 127>),
    /// Channel II's AL, the filter envelope's attack level: controller 78, 0 to 127.
    channel_2_filter_attack_level(U8In<0, 127>),
    /// Channel II's Filter envelope A: controller 79, 0 to 127.
    channel_2_filter_attack(U8In<0, 127>),
    /// Channel II's Filter envelope D: controller 80, 0 to 127.
    channel_2_filter_decay(U8In<0, 127>),
    /// Channel II's Filter envelope R: controller 81, 0 to 127.
    channel_2_filter_release(U8In<0, 127>),
    /// Channel II's VCF LEVEL: controller 82, 0 to 127.
    channel_2_vcf_level(U8In<0, 127>),
    /// Channel II's SINE: controller 83, 0 to 127.
    channel_2_sine(U8In<0, 127>),
    /// Channel II's Amplifier envelope A: controller 84, 0 to 127.
    channel_2_amplifier_attack(U8In<0, 127>),
    /// Channel II's Amplifier envelope D: controller 85, 0 to 127.
    channel_2_amplifier_decay(U8In<0, 127>),
    /// Channel II's Amplifier envelope S: controller 86, 0 to 127.
    channel_2_amplifier_sustain(U8In<0, 127>),
    /// Channel II's Amplifier envelope R: controller 87, 0 to 127.
    channel_2_amplifier_release(U8In<0, 127>),
    /// Channel II's LEVEL: controller 88, 0 to 127.
    channel_2_level(U8In<0, 127>),
    /// Channel II's INITIAL BRILLIANCE: controller 89, 0 to 127.
    channel_2_initial_brilliance(U8In<0, 127>),
    /// Channel II's INITIAL LEVEL: controller 90, 0 to 127.
    channel_2_initial_level(U8In<0, 127>),
    /// Channel II's AFTER BRILLIANCE: controller 91, 0 to 127.
    channel_2_after_brilliance(U8In<0, 127>),
    /// Channel II's AFTER LEVEL: controller 92, 0 to 127.
    channel_2_after_level(U8In<0, 127>),
}

impl Synth80Parameter {
    /// Every parameter's name, in the order of the family document's table.
    pub const NAMES: [&'static str; 85] = [
        "feet_1",
        "feet_2",
        "detune_channel_2",
        "mix",
        "brilliance",
        "resonance",
        "keyboard_brilliance_low",
        "keyboard_brilliance_high",
        "keyboard_level_low",
        "keyboard_level_high",
        "sub_oscillator_function",
        "sub_oscillator_speed",
        "sub_oscillator_vco",
        "sub_oscillator_vcf",
        "sub_oscillator_vca",
        "touch_pitch_bend",
        "touch_speed",
        "touch_vco",
        "touch_vcf",
        "ring_modulator_attack_time",
        "ring_modulator_decay_time",
        "ring_modulator_depth",
        "ring_modulator_speed",
        "ring_modulator_modulation",
        "sustain_mode",
        "sustain",
        "glide_mode",
        "glide_speed",
        "tremolo_chorus",
        "chorus",
        "tremolo",
        "tremolo_chorus_speed",
        "tremolo_chorus_depth",
        "channel_1_pwm_speed",
        "channel_1_pwm",
        "channel_1_pw",
        "channel_1_square",
        "channel_1_sawtooth",
        "channel_1_noise",
        "channel_1_hpf",
        "channel_1_hpf_resonance",
        "channel_1_lpf",
        "channel_1_lpf_resonance",
        "channel_1_filter_initial_level",
        "channel_1_filter_attack_level",
        "channel_1_filter_attack",
        "channel_1_filter_decay",
        "channel_1_filter_release",
        "channel_1_vcf_level",
        "channel_1_sine",
        "channel_1_amplifier_attack",
        "channel_1_amplifier_decay",
        "channel_1_amplifier_sustain",
        "channel_1_amplifier_release",
        "channel_1_level",
        "channel_1_initial_brilliance",
        "channel_1_initial_level",
        "channel_1_after_brilliance",
        "channel_1_after_level",
        "channel_2_pwm_speed",
        "channel_2_pwm",
        "channel_2_pw",
        "channel_2_square",
        "channel_2_sawtooth",
        "channel_2_noise",
        "channel_2_hpf",
        "channel_2_hpf_resonance",
        "channel_2_lpf",
        "channel_2_lpf_resonance",
        "channel_2_filter_initial_level",
        "channel_2_filter_attack_level",
        "channel_2_filter_attack",
        "channel_2_filter_decay",
        "channel_2_filter_release",
        "channel_2_vcf_level",
        "channel_2_sine",
        "channel_2_amplifier_attack",
        "channel_2_amplifier_decay",
        "channel_2_amplifier_sustain",
        "channel_2_amplifier_release",
        "channel_2_level",
        "channel_2_initial_brilliance",
        "channel_2_initial_level",
        "channel_2_after_brilliance",
        "channel_2_after_level",
    ];

    /// The parameter's name.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one arm for each of the family's named parameters")]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::feet_1(_) => "feet_1",
            Self::feet_2(_) => "feet_2",
            Self::detune_channel_2(_) => "detune_channel_2",
            Self::mix(_) => "mix",
            Self::brilliance(_) => "brilliance",
            Self::resonance(_) => "resonance",
            Self::keyboard_brilliance_low(_) => "keyboard_brilliance_low",
            Self::keyboard_brilliance_high(_) => "keyboard_brilliance_high",
            Self::keyboard_level_low(_) => "keyboard_level_low",
            Self::keyboard_level_high(_) => "keyboard_level_high",
            Self::sub_oscillator_function(_) => "sub_oscillator_function",
            Self::sub_oscillator_speed(_) => "sub_oscillator_speed",
            Self::sub_oscillator_vco(_) => "sub_oscillator_vco",
            Self::sub_oscillator_vcf(_) => "sub_oscillator_vcf",
            Self::sub_oscillator_vca(_) => "sub_oscillator_vca",
            Self::touch_pitch_bend(_) => "touch_pitch_bend",
            Self::touch_speed(_) => "touch_speed",
            Self::touch_vco(_) => "touch_vco",
            Self::touch_vcf(_) => "touch_vcf",
            Self::ring_modulator_attack_time(_) => "ring_modulator_attack_time",
            Self::ring_modulator_decay_time(_) => "ring_modulator_decay_time",
            Self::ring_modulator_depth(_) => "ring_modulator_depth",
            Self::ring_modulator_speed(_) => "ring_modulator_speed",
            Self::ring_modulator_modulation(_) => "ring_modulator_modulation",
            Self::sustain_mode(_) => "sustain_mode",
            Self::sustain(_) => "sustain",
            Self::glide_mode(_) => "glide_mode",
            Self::glide_speed(_) => "glide_speed",
            Self::tremolo_chorus(_) => "tremolo_chorus",
            Self::chorus(_) => "chorus",
            Self::tremolo(_) => "tremolo",
            Self::tremolo_chorus_speed(_) => "tremolo_chorus_speed",
            Self::tremolo_chorus_depth(_) => "tremolo_chorus_depth",
            Self::channel_1_pwm_speed(_) => "channel_1_pwm_speed",
            Self::channel_1_pwm(_) => "channel_1_pwm",
            Self::channel_1_pw(_) => "channel_1_pw",
            Self::channel_1_square(_) => "channel_1_square",
            Self::channel_1_sawtooth(_) => "channel_1_sawtooth",
            Self::channel_1_noise(_) => "channel_1_noise",
            Self::channel_1_hpf(_) => "channel_1_hpf",
            Self::channel_1_hpf_resonance(_) => "channel_1_hpf_resonance",
            Self::channel_1_lpf(_) => "channel_1_lpf",
            Self::channel_1_lpf_resonance(_) => "channel_1_lpf_resonance",
            Self::channel_1_filter_initial_level(_) => "channel_1_filter_initial_level",
            Self::channel_1_filter_attack_level(_) => "channel_1_filter_attack_level",
            Self::channel_1_filter_attack(_) => "channel_1_filter_attack",
            Self::channel_1_filter_decay(_) => "channel_1_filter_decay",
            Self::channel_1_filter_release(_) => "channel_1_filter_release",
            Self::channel_1_vcf_level(_) => "channel_1_vcf_level",
            Self::channel_1_sine(_) => "channel_1_sine",
            Self::channel_1_amplifier_attack(_) => "channel_1_amplifier_attack",
            Self::channel_1_amplifier_decay(_) => "channel_1_amplifier_decay",
            Self::channel_1_amplifier_sustain(_) => "channel_1_amplifier_sustain",
            Self::channel_1_amplifier_release(_) => "channel_1_amplifier_release",
            Self::channel_1_level(_) => "channel_1_level",
            Self::channel_1_initial_brilliance(_) => "channel_1_initial_brilliance",
            Self::channel_1_initial_level(_) => "channel_1_initial_level",
            Self::channel_1_after_brilliance(_) => "channel_1_after_brilliance",
            Self::channel_1_after_level(_) => "channel_1_after_level",
            Self::channel_2_pwm_speed(_) => "channel_2_pwm_speed",
            Self::channel_2_pwm(_) => "channel_2_pwm",
            Self::channel_2_pw(_) => "channel_2_pw",
            Self::channel_2_square(_) => "channel_2_square",
            Self::channel_2_sawtooth(_) => "channel_2_sawtooth",
            Self::channel_2_noise(_) => "channel_2_noise",
            Self::channel_2_hpf(_) => "channel_2_hpf",
            Self::channel_2_hpf_resonance(_) => "channel_2_hpf_resonance",
            Self::channel_2_lpf(_) => "channel_2_lpf",
            Self::channel_2_lpf_resonance(_) => "channel_2_lpf_resonance",
            Self::channel_2_filter_initial_level(_) => "channel_2_filter_initial_level",
            Self::channel_2_filter_attack_level(_) => "channel_2_filter_attack_level",
            Self::channel_2_filter_attack(_) => "channel_2_filter_attack",
            Self::channel_2_filter_decay(_) => "channel_2_filter_decay",
            Self::channel_2_filter_release(_) => "channel_2_filter_release",
            Self::channel_2_vcf_level(_) => "channel_2_vcf_level",
            Self::channel_2_sine(_) => "channel_2_sine",
            Self::channel_2_amplifier_attack(_) => "channel_2_amplifier_attack",
            Self::channel_2_amplifier_decay(_) => "channel_2_amplifier_decay",
            Self::channel_2_amplifier_sustain(_) => "channel_2_amplifier_sustain",
            Self::channel_2_amplifier_release(_) => "channel_2_amplifier_release",
            Self::channel_2_level(_) => "channel_2_level",
            Self::channel_2_initial_brilliance(_) => "channel_2_initial_brilliance",
            Self::channel_2_initial_level(_) => "channel_2_initial_level",
            Self::channel_2_after_brilliance(_) => "channel_2_after_brilliance",
            Self::channel_2_after_level(_) => "channel_2_after_level",
        }
    }
}

/// The 28 registers of an ASID register data message by name, each
/// present when given, any value 0 to 255 (synth6581-synth8580.md, S6581-11,
/// S6581-17; server.md SRV-12), named for the ASID register IDs in ID order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth6581Registers {
    /// ASID register ID 0.
    #[serde(default)]
    pub voice1_frequency_low: Option<u8>,
    /// ASID register ID 1.
    #[serde(default)]
    pub voice1_frequency_high: Option<u8>,
    /// ASID register ID 2.
    #[serde(default)]
    pub voice1_pulse_width_low: Option<u8>,
    /// ASID register ID 3.
    #[serde(default)]
    pub voice1_pulse_width_high: Option<u8>,
    /// ASID register ID 4.
    #[serde(default)]
    pub voice1_attack_decay: Option<u8>,
    /// ASID register ID 5.
    #[serde(default)]
    pub voice1_sustain_release: Option<u8>,
    /// ASID register ID 6.
    #[serde(default)]
    pub voice2_frequency_low: Option<u8>,
    /// ASID register ID 7.
    #[serde(default)]
    pub voice2_frequency_high: Option<u8>,
    /// ASID register ID 8.
    #[serde(default)]
    pub voice2_pulse_width_low: Option<u8>,
    /// ASID register ID 9.
    #[serde(default)]
    pub voice2_pulse_width_high: Option<u8>,
    /// ASID register ID 10.
    #[serde(default)]
    pub voice2_attack_decay: Option<u8>,
    /// ASID register ID 11.
    #[serde(default)]
    pub voice2_sustain_release: Option<u8>,
    /// ASID register ID 12.
    #[serde(default)]
    pub voice3_frequency_low: Option<u8>,
    /// ASID register ID 13.
    #[serde(default)]
    pub voice3_frequency_high: Option<u8>,
    /// ASID register ID 14.
    #[serde(default)]
    pub voice3_pulse_width_low: Option<u8>,
    /// ASID register ID 15.
    #[serde(default)]
    pub voice3_pulse_width_high: Option<u8>,
    /// ASID register ID 16.
    #[serde(default)]
    pub voice3_attack_decay: Option<u8>,
    /// ASID register ID 17.
    #[serde(default)]
    pub voice3_sustain_release: Option<u8>,
    /// ASID register ID 18.
    #[serde(default)]
    pub filter_cutoff_low: Option<u8>,
    /// ASID register ID 19.
    #[serde(default)]
    pub filter_cutoff_high: Option<u8>,
    /// ASID register ID 20.
    #[serde(default)]
    pub filter_resonance_routing: Option<u8>,
    /// ASID register ID 21.
    #[serde(default)]
    pub filter_mode_volume: Option<u8>,
    /// ASID register ID 22.
    #[serde(default)]
    pub voice1_control: Option<u8>,
    /// ASID register ID 23.
    #[serde(default)]
    pub voice2_control: Option<u8>,
    /// ASID register ID 24.
    #[serde(default)]
    pub voice3_control: Option<u8>,
    /// ASID register ID 25.
    #[serde(default)]
    pub voice1_control_second: Option<u8>,
    /// ASID register ID 26.
    #[serde(default)]
    pub voice2_control_second: Option<u8>,
    /// ASID register ID 27.
    #[serde(default)]
    pub voice3_control_second: Option<u8>,
}

impl Synth6581Registers {
    /// The values by ASID register ID, 0 to 27: what the engine's register
    /// data message is built from.
    #[must_use]
    pub const fn by_id(&self) -> [Option<u8>; 28] {
        [
            self.voice1_frequency_low,
            self.voice1_frequency_high,
            self.voice1_pulse_width_low,
            self.voice1_pulse_width_high,
            self.voice1_attack_decay,
            self.voice1_sustain_release,
            self.voice2_frequency_low,
            self.voice2_frequency_high,
            self.voice2_pulse_width_low,
            self.voice2_pulse_width_high,
            self.voice2_attack_decay,
            self.voice2_sustain_release,
            self.voice3_frequency_low,
            self.voice3_frequency_high,
            self.voice3_pulse_width_low,
            self.voice3_pulse_width_high,
            self.voice3_attack_decay,
            self.voice3_sustain_release,
            self.filter_cutoff_low,
            self.filter_cutoff_high,
            self.filter_resonance_routing,
            self.filter_mode_volume,
            self.voice1_control,
            self.voice2_control,
            self.voice3_control,
            self.voice1_control_second,
            self.voice2_control_second,
            self.voice3_control_second,
        ]
    }
}
