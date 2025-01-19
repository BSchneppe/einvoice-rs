use crate::{BusinessRuleViolation, CrossIndustryInvoice, ValidationError, ZugferdProfile};
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use std::sync::Arc;

pub fn validate_invoice(
    zugferd_profile: ZugferdProfile,
    invoice: Arc<&CrossIndustryInvoice>,
) -> Result<(), Vec<ValidationError>> {
    let validation_rules = match zugferd_profile {
        ZugferdProfile::Basic => BASIC_BUSINESS_RULES,
        ZugferdProfile::En16931 => EN16931_BUSINESS_RULES,
        ZugferdProfile::Extended => &*vec![],
    };

    let results: Vec<_> = validation_rules
        .par_iter()
        .map(|rule| rule(&invoice))
        .filter_map(Result::err) // Collect only errors
        .collect();

    if results.is_empty() {
        Ok(())
    } else {
        Err(results)
    }
}

type BusinessRule = fn(&CrossIndustryInvoice) -> Result<(), ValidationError>;

pub const BASIC_BUSINESS_RULES: &[BusinessRule] = &[
    validate_br_45,
    validate_br_46,
    validate_br_47,
    validate_br_48,
    validate_br_co_03,
    validate_br_co_17,
    validate_br_dec_19,
    validate_br_dec_20,
    validate_br_z_08,
    validate_br_z_09,
    validate_br_z_10,
    validate_br_s_09,
    validate_br_s_10,
    validate_br_s_08,
    validate_br_29,
    validate_br_co_19,
    validate_br_31,
    validate_br_32,
    validate_br_33,
    validate_br_co_05,
    validate_br_co_21,
    validate_br_dec_01,
    validate_br_dec_02,
    validate_br_36,
    validate_br_37,
    validate_br_38,
    validate_br_co_06,
    validate_br_co_22,
    validate_br_dec_05,
    validate_br_dec_06,
    validate_br_54,
    validate_br_21,
    validate_br_22,
    validate_br_23,
    validate_br_24,
    validate_br_25,
    validate_br_26,
    validate_br_27,
    validate_br_28,
    validate_br_64,
    validate_br_65,
    validate_br_co_04,
    validate_br_dec_23,
    validate_br_17,
    validate_br_18,
    validate_br_19,
    validate_br_20,
    validate_br_56,
    validate_br_co_26,
    validate_br_30,
    validate_br_co_20,
    validate_br_42,
    validate_br_41,
    validate_br_co_07,
    validate_br_co_23,
    validate_br_dec_24,
    validate_br_dec_25,
    validate_br_43,
    validate_br_44,
    validate_br_co_08,
    validate_br_co_24,
    validate_br_dec_27,
    validate_br_dec_28,
    validate_br_co_09,
    validate_br_66,
    validate_br_ae_03,
    validate_br_ae_06,
    validate_br_e_03,
    validate_br_e_06,
    validate_br_g_03,
    validate_br_g_06,
    validate_br_ic_03,
    validate_br_ic_06,
    validate_br_af_03,
    validate_br_af_06,
    validate_br_ag_03,
    validate_br_ag_06,
    validate_br_o_03,
    validate_br_o_06,
    validate_br_s_03,
    validate_br_s_06,
    validate_br_z_03,
    validate_br_z_06,
    validate_br_ae_04,
    validate_br_ae_07,
    validate_br_e_04,
    validate_br_e_07,
    validate_br_g_04,
    validate_br_g_07,
    validate_br_ic_04,
    validate_br_ic_07,
    validate_br_af_04,
    validate_br_af_07,
    validate_br_ag_04,
    validate_br_ag_07,
    validate_br_o_04,
    validate_br_o_07,
    validate_br_s_04,
    validate_br_s_07,
    validate_br_z_04,
    validate_br_z_07,
    validate_br_co_10,
    validate_br_12,
    validate_br_13,
    validate_br_14,
    validate_br_15,
    validate_br_co_11,
    validate_br_co_12,
    validate_br_co_13,
    validate_br_co_15,
    validate_br_co_16,
    validate_br_dec_09,
    validate_br_dec_10,
    validate_br_dec_11,
    validate_br_dec_12,
    validate_br_dec_13,
    validate_br_dec_14,
    validate_br_dec_15,
    validate_br_dec_16,
    validate_br_dec_17,
    validate_br_dec_18,
    validate_br_53,
    validate_br_co_14,
    validate_br_49,
    validate_br_co_27,
    validate_br_61,
    validate_br_50,
    validate_br_co_18,
    validate_br_ae_08,
    validate_br_ae_09,
    validate_br_ae_10,
    validate_br_e_08,
    validate_br_e_09,
    validate_br_e_10,
    validate_br_g_08,
    validate_br_g_09,
    validate_br_g_10,
    validate_br_ic_08,
    validate_br_ic_09,
    validate_br_ic_10,
    validate_br_ic_11,
    validate_br_ic_12,
    validate_br_af_08,
    validate_br_af_09,
    validate_br_af_10,
    validate_br_ag_08,
    validate_br_ag_09,
    validate_br_ag_10,
    validate_br_o_08,
    validate_br_o_09,
    validate_br_o_10,
    validate_br_o_11,
    validate_br_o_12,
    validate_br_o_13,
    validate_br_o_14,
    validate_br_ae_02,
    validate_br_ae_05,
    validate_br_e_02,
    validate_br_e_05,
    validate_br_g_02,
    validate_br_g_05,
    validate_br_ic_02,
    validate_br_ic_05,
    validate_br_af_02,
    validate_br_af_05,
    validate_br_ag_02,
    validate_br_ag_05,
    validate_br_o_02,
    validate_br_o_05,
    validate_br_s_02,
    validate_br_s_05,
    validate_br_z_02,
    validate_br_z_05,
    validate_br_16,
    validate_br_co_25,
    validate_br_01,
    validate_br_02,
    validate_br_03,
    validate_br_04,
    validate_br_05,
    validate_br_06,
    validate_br_07,
    validate_br_08,
    validate_br_09,
    validate_br_10,
    validate_br_11,
    validate_br_62,
    validate_br_63,
    validate_br_s_01,
    validate_br_z_01,
    validate_br_e_01,
    validate_br_ae_01,
    validate_br_ic_01,
    validate_br_g_01,
    validate_br_o_01,
    validate_br_af_01,
    validate_br_ag_01,
    validate_br_b_01,
    validate_br_b_02,
    validate_br_57,
    validate_br_55,
];
pub const EN16931_BUSINESS_RULES: &[BusinessRule] = &[
    validate_br_66,
    validate_br_65,
    validate_br_52,
    validate_br_51,
    validate_br_50,
    validate_br_61,
    validate_br_57,
    validate_br_31,
    validate_br_32,
    validate_br_33,
    validate_br_co_05,
    validate_br_co_21,
    validate_br_dec_01,
    validate_br_dec_02,
    validate_br_36,
    validate_br_37,
    validate_br_38,
    validate_br_co_06,
    validate_br_co_22,
    validate_br_dec_05,
    validate_br_dec_06,
    validate_br_12,
    validate_br_13,
    validate_br_14,
    validate_br_15,
    validate_br_53,
    validate_br_co_10,
    validate_br_co_11,
    validate_br_co_12,
    validate_br_co_13,
    validate_br_co_16,
    validate_br_dec_09,
    validate_br_dec_10,
    validate_br_dec_11,
    validate_br_dec_12,
    validate_br_dec_14,
    validate_br_dec_13,
    validate_br_dec_15,
    validate_br_dec_16,
    validate_br_dec_17,
    validate_br_dec_18,
    validate_br_01,
    validate_br_02,
    validate_br_03,
    validate_br_04,
    validate_br_05,
    validate_br_06,
    validate_br_07,
    validate_br_08,
    validate_br_09,
    validate_br_10,
    validate_br_11,
    validate_br_16,
    validate_br_62,
    validate_br_63,
    validate_br_co_15,
    validate_br_co_25,
    validate_br_s_01,
    validate_br_z_01,
    validate_br_e_01,
    validate_br_ae_01,
    validate_br_ic_01,
    validate_br_g_01,
    validate_br_o_01,
    validate_br_af_01,
    validate_br_ag_01,
    validate_br_b_01,
    validate_br_b_02,
    validate_br_21,
    validate_br_22,
    validate_br_23,
    validate_br_24,
    validate_br_25,
    validate_br_26,
    validate_br_27,
    validate_br_28,
    validate_br_64,
    validate_br_co_04,
    validate_br_co_18,
    validate_br_dec_23,
    validate_br_41,
    validate_br_42,
    validate_br_co_07,
    validate_br_co_23,
    validate_br_dec_24,
    validate_br_dec_25,
    validate_br_43,
    validate_br_44,
    validate_br_co_08,
    validate_br_co_24,
    validate_br_dec_27,
    validate_br_dec_28,
    validate_br_30,
    validate_br_co_20,
    validate_br_29,
    validate_br_co_19,
    validate_br_54,
    validate_br_17,
    validate_br_49,
    validate_br_co_27,
    validate_br_55,
    validate_br_co_26,
    validate_br_18,
    validate_br_19,
    validate_br_20,
    validate_br_56,
    validate_br_co_14,
    validate_br_co_09,
    validate_br_ae_08,
    validate_br_ae_09,
    validate_br_ae_10,
    validate_br_ae_03,
    validate_br_ae_06,
    validate_br_ae_04,
    validate_br_ae_07,
    validate_br_ae_02,
    validate_br_ae_05,
    validate_br_af_08,
    validate_br_af_09,
    validate_br_af_10,
    validate_br_af_02,
    validate_br_af_05,
    validate_br_af_03,
    validate_br_af_06,
    validate_br_af_04,
    validate_br_af_07,
    validate_br_ag_08,
    validate_br_ag_09,
    validate_br_ag_10,
    validate_br_ag_02,
    validate_br_ag_05,
    validate_br_ag_03,
    validate_br_ag_06,
    validate_br_ag_04,
    validate_br_ag_07,
    validate_br_e_08,
    validate_br_e_09,
    validate_br_e_10,
    validate_br_e_03,
    validate_br_e_06,
    validate_br_e_04,
    validate_br_e_07,
    validate_br_e_02,
    validate_br_e_05,
    validate_br_g_08,
    validate_br_g_09,
    validate_br_g_10,
    validate_br_g_03,
    validate_br_g_06,
    validate_br_g_04,
    validate_br_g_07,
    validate_br_g_02,
    validate_br_g_05,
    validate_br_ic_08,
    validate_br_ic_09,
    validate_br_ic_10,
    validate_br_ic_11,
    validate_br_ic_12,
    validate_br_ic_03,
    validate_br_ic_06,
    validate_br_ic_04,
    validate_br_ic_07,
    validate_br_ic_02,
    validate_br_ic_05,
    validate_br_o_08,
    validate_br_o_09,
    validate_br_o_10,
    validate_br_o_11,
    validate_br_o_12,
    validate_br_o_13,
    validate_br_o_14,
    validate_br_o_03,
    validate_br_o_06,
    validate_br_o_04,
    validate_br_o_07,
    validate_br_o_02,
    validate_br_o_05,
    validate_br_s_08,
    validate_br_s_09,
    validate_br_s_10,
    validate_br_s_02,
    validate_br_s_05,
    validate_br_s_03,
    validate_br_s_06,
    validate_br_s_04,
    validate_br_s_07,
    validate_br_z_08,
    validate_br_z_09,
    validate_br_z_10,
    validate_br_z_03,
    validate_br_z_06,
    validate_br_z_04,
    validate_br_z_07,
    validate_br_z_02,
    validate_br_z_05,
    validate_br_45,
    validate_br_46,
    validate_br_47,
    validate_br_48,
    validate_br_co_03,
    validate_br_co_17,
    validate_br_dec_19,
    validate_br_dec_20,
    validate_cii_sr_001,
    validate_cii_sr_002,
    validate_cii_sr_003,
    validate_cii_sr_006,
    validate_cii_sr_007,
    validate_cii_sr_008,
    validate_cii_sr_009,
    validate_cii_sr_010,
    validate_cii_sr_011,
    validate_cii_sr_012,
    validate_cii_sr_013,
    validate_cii_sr_014,
    validate_cii_sr_015,
    validate_cii_sr_016,
    validate_cii_sr_017,
    validate_cii_sr_018,
    validate_cii_sr_019,
    validate_cii_sr_020,
    validate_cii_sr_021,
    validate_cii_sr_022,
    validate_cii_sr_023,
    validate_cii_sr_024,
    validate_cii_sr_025,
    validate_cii_sr_026,
    validate_cii_sr_027,
    validate_cii_sr_028,
    validate_cii_sr_032,
    validate_cii_sr_033,
    validate_cii_sr_034,
    validate_cii_sr_030,
    validate_cii_sr_035,
    validate_cii_sr_036,
    validate_cii_sr_037,
    validate_cii_sr_038,
    validate_cii_sr_221,
    validate_cii_sr_039,
    validate_cii_sr_040,
    validate_cii_sr_041,
    validate_cii_sr_042,
    validate_cii_sr_043,
    validate_cii_sr_044,
    validate_cii_sr_045,
    validate_cii_sr_046,
    validate_cii_sr_048,
    validate_cii_sr_049,
    validate_cii_sr_050,
    validate_cii_sr_051,
    validate_cii_sr_052,
    validate_cii_sr_053,
    validate_cii_sr_054,
    validate_cii_sr_055,
    validate_cii_sr_056,
    validate_cii_sr_057,
    validate_cii_sr_058,
    validate_cii_sr_059,
    validate_cii_sr_060,
    validate_cii_sr_061,
    validate_cii_sr_062,
    validate_cii_sr_063,
    validate_cii_sr_064,
    validate_cii_sr_065,
    validate_cii_sr_066,
    validate_cii_sr_067,
    validate_cii_sr_068,
    validate_cii_sr_070,
    validate_cii_sr_071,
    validate_cii_sr_073,
    validate_cii_sr_074,
    validate_cii_sr_075,
    validate_cii_sr_076,
    validate_cii_sr_077,
    validate_cii_sr_078,
    validate_cii_sr_079,
    validate_cii_sr_080,
    validate_cii_sr_081,
    validate_cii_sr_082,
    validate_cii_sr_083,
    validate_cii_sr_084,
    validate_cii_sr_085,
    validate_cii_sr_086,
    validate_cii_sr_087,
    validate_cii_sr_088,
    validate_cii_sr_089,
    validate_cii_sr_090,
    validate_cii_sr_091,
    validate_cii_sr_092,
    validate_cii_sr_093,
    validate_cii_sr_094,
    validate_cii_sr_095,
    validate_cii_sr_096,
    validate_cii_sr_097,
    validate_cii_sr_098,
    validate_cii_sr_099,
    validate_cii_sr_100,
    validate_cii_sr_101,
    validate_cii_sr_102,
    validate_cii_sr_103,
    validate_cii_sr_069,
    validate_cii_sr_072,
    validate_cii_sr_104,
    validate_cii_sr_105,
    validate_cii_sr_106,
    validate_cii_sr_107,
    validate_cii_sr_108,
    validate_cii_sr_109,
    validate_cii_sr_110,
    validate_cii_sr_111,
    validate_cii_sr_112,
    validate_cii_sr_113,
    validate_cii_sr_114,
    validate_cii_sr_115,
    validate_cii_sr_116,
    validate_cii_sr_117,
    validate_cii_sr_118,
    validate_cii_sr_439,
    validate_cii_sr_119,
    validate_cii_sr_120,
    validate_cii_sr_121,
    validate_cii_sr_122,
    validate_cii_sr_123,
    validate_cii_sr_124,
    validate_cii_sr_125,
    validate_cii_sr_126,
    validate_cii_sr_127,
    validate_cii_sr_128,
    validate_cii_sr_129,
    validate_cii_sr_130,
    validate_cii_sr_131,
    validate_cii_sr_445,
    validate_cii_sr_132,
    validate_cii_sr_133,
    validate_cii_sr_134,
    validate_cii_sr_135,
    validate_cii_sr_136,
    validate_cii_sr_138,
    validate_cii_sr_139,
    validate_cii_sr_140,
    validate_cii_sr_141,
    validate_cii_sr_142,
    validate_cii_sr_446,
    validate_cii_sr_143,
    validate_cii_sr_144,
    validate_cii_sr_145,
    validate_cii_sr_146,
    validate_cii_sr_441,
    validate_cii_sr_147,
    validate_cii_sr_148,
    validate_cii_sr_149,
    validate_cii_sr_150,
    validate_cii_sr_447,
    validate_cii_sr_440,
    validate_cii_sr_151,
    validate_cii_sr_152,
    validate_cii_sr_153,
    validate_cii_sr_154,
    validate_cii_sr_155,
    validate_cii_sr_156,
    validate_cii_sr_157,
    validate_cii_sr_158,
    validate_cii_sr_159,
    validate_cii_sr_160,
    validate_cii_sr_161,
    validate_cii_sr_162,
    validate_cii_sr_163,
    validate_cii_sr_164,
    validate_cii_sr_165,
    validate_cii_sr_166,
    validate_cii_sr_167,
    validate_cii_sr_168,
    validate_cii_sr_169,
    validate_cii_sr_170,
    validate_cii_sr_171,
    validate_cii_sr_172,
    validate_cii_sr_173,
    validate_cii_sr_174,
    validate_cii_sr_175,
    validate_cii_sr_176,
    validate_cii_sr_177,
    validate_cii_sr_178,
    validate_cii_sr_179,
    validate_cii_sr_180,
    validate_cii_sr_181,
    validate_cii_sr_182,
    validate_cii_sr_183,
    validate_cii_sr_184,
    validate_cii_sr_185,
    validate_cii_sr_186,
    validate_cii_sr_187,
    validate_cii_sr_188,
    validate_cii_sr_189,
    validate_cii_sr_190,
    validate_cii_sr_191,
    validate_cii_sr_192,
    validate_cii_sr_193,
    validate_cii_sr_194,
    validate_cii_sr_195,
    validate_cii_sr_196,
    validate_cii_sr_197,
    validate_cii_sr_198,
    validate_cii_sr_199,
    validate_cii_sr_200,
    validate_cii_sr_201,
    validate_cii_sr_202,
    validate_cii_sr_203,
    validate_cii_sr_204,
    validate_cii_sr_205,
    validate_cii_sr_206,
    validate_cii_sr_207,
    validate_cii_sr_208,
    validate_cii_sr_209,
    validate_cii_sr_210,
    validate_cii_sr_212,
    validate_cii_sr_213,
    validate_cii_sr_214,
    validate_cii_sr_215,
    validate_cii_sr_216,
    validate_cii_sr_217,
    validate_cii_sr_218,
    validate_cii_sr_219,
    validate_cii_sr_220,
    validate_cii_sr_454,
    validate_cii_sr_442,
    validate_cii_sr_222,
    validate_cii_sr_223,
    validate_cii_sr_224,
    validate_cii_sr_225,
    validate_cii_sr_226,
    validate_cii_sr_227,
    validate_cii_sr_228,
    validate_cii_sr_229,
    validate_cii_sr_230,
    validate_cii_sr_231,
    validate_cii_sr_232,
    validate_cii_sr_233,
    validate_cii_sr_234,
    validate_cii_sr_235,
    validate_cii_sr_236,
    validate_cii_sr_237,
    validate_cii_sr_238,
    validate_cii_sr_239,
    validate_cii_sr_240,
    validate_cii_sr_241,
    validate_cii_sr_242,
    validate_cii_sr_243,
    validate_cii_sr_244,
    validate_cii_sr_245,
    validate_cii_sr_246,
    validate_cii_sr_247,
    validate_cii_sr_248,
    validate_cii_sr_249,
    validate_cii_sr_250,
    validate_cii_sr_251,
    validate_cii_sr_252,
    validate_cii_sr_254,
    validate_cii_sr_255,
    validate_cii_sr_256,
    validate_cii_sr_257,
    validate_cii_sr_258,
    validate_cii_sr_259,
    validate_cii_sr_260,
    validate_cii_sr_261,
    validate_cii_sr_262,
    validate_cii_sr_263,
    validate_cii_sr_264,
    validate_cii_sr_265,
    validate_cii_sr_266,
    validate_cii_sr_267,
    validate_cii_sr_268,
    validate_cii_sr_269,
    validate_cii_sr_270,
    validate_cii_sr_271,
    validate_cii_sr_272,
    validate_cii_sr_273,
    validate_cii_sr_274,
    validate_cii_sr_275,
    validate_cii_sr_276,
    validate_cii_sr_277,
    validate_cii_sr_278,
    validate_cii_sr_279,
    validate_cii_sr_280,
    validate_cii_sr_281,
    validate_cii_sr_282,
    validate_cii_sr_283,
    validate_cii_sr_284,
    validate_cii_sr_285,
    validate_cii_sr_286,
    validate_cii_sr_287,
    validate_cii_sr_288,
    validate_cii_sr_289,
    validate_cii_sr_290,
    validate_cii_sr_291,
    validate_cii_sr_292,
    validate_cii_sr_293,
    validate_cii_sr_294,
    validate_cii_sr_295,
    validate_cii_sr_296,
    validate_cii_sr_297,
    validate_cii_sr_298,
    validate_cii_sr_299,
    validate_cii_sr_300,
    validate_cii_sr_301,
    validate_cii_sr_302,
    validate_cii_sr_303,
    validate_cii_sr_304,
    validate_cii_sr_305,
    validate_cii_sr_306,
    validate_cii_sr_307,
    validate_cii_sr_448,
    validate_cii_sr_450,
    validate_cii_sr_455,
    validate_cii_sr_456,
    validate_cii_sr_457,
    validate_cii_sr_458,
    validate_cii_sr_459,
    validate_cii_sr_460,
    validate_cii_sr_308,
    validate_cii_sr_309,
    validate_cii_sr_310,
    validate_cii_sr_311,
    validate_cii_sr_312,
    validate_cii_sr_313,
    validate_cii_sr_314,
    validate_cii_sr_315,
    validate_cii_sr_316,
    validate_cii_sr_317,
    validate_cii_sr_318,
    validate_cii_sr_319,
    validate_cii_sr_320,
    validate_cii_sr_321,
    validate_cii_sr_322,
    validate_cii_sr_323,
    validate_cii_sr_324,
    validate_cii_sr_325,
    validate_cii_sr_326,
    validate_cii_sr_327,
    validate_cii_sr_328,
    validate_cii_sr_329,
    validate_cii_sr_330,
    validate_cii_sr_331,
    validate_cii_sr_332,
    validate_cii_sr_333,
    validate_cii_sr_334,
    validate_cii_sr_335,
    validate_cii_sr_336,
    validate_cii_sr_337,
    validate_cii_sr_338,
    validate_cii_sr_449,
    validate_cii_sr_339,
    validate_cii_sr_340,
    validate_cii_sr_341,
    validate_cii_sr_342,
    validate_cii_sr_344,
    validate_cii_sr_345,
    validate_cii_sr_346,
    validate_cii_sr_347,
    validate_cii_sr_348,
    validate_cii_sr_349,
    validate_cii_sr_350,
    validate_cii_sr_351,
    validate_cii_sr_352,
    validate_cii_sr_353,
    validate_cii_sr_354,
    validate_cii_sr_355,
    validate_cii_sr_356,
    validate_cii_sr_357,
    validate_cii_sr_358,
    validate_cii_sr_359,
    validate_cii_sr_360,
    validate_cii_sr_361,
    validate_cii_sr_362,
    validate_cii_sr_363,
    validate_cii_sr_364,
    validate_cii_sr_451,
    validate_cii_sr_365,
    validate_cii_sr_366,
    validate_cii_sr_367,
    validate_cii_sr_368,
    validate_cii_sr_369,
    validate_cii_sr_370,
    validate_cii_sr_371,
    validate_cii_sr_443,
    validate_cii_sr_372,
    validate_cii_sr_373,
    validate_cii_sr_375,
    validate_cii_sr_376,
    validate_cii_sr_377,
    validate_cii_sr_378,
    validate_cii_sr_379,
    validate_cii_sr_380,
    validate_cii_sr_381,
    validate_cii_sr_382,
    validate_cii_sr_444,
    validate_cii_sr_384,
    validate_cii_sr_385,
    validate_cii_sr_386,
    validate_cii_sr_388,
    validate_cii_sr_389,
    validate_cii_sr_390,
    validate_cii_sr_391,
    validate_cii_sr_392,
    validate_cii_sr_393,
    validate_cii_sr_394,
    validate_cii_sr_395,
    validate_cii_sr_396,
    validate_cii_sr_397,
    validate_cii_sr_398,
    validate_cii_sr_399,
    validate_cii_sr_400,
    validate_cii_sr_401,
    validate_cii_sr_402,
    validate_cii_sr_404,
    validate_cii_sr_405,
    validate_cii_sr_406,
    validate_cii_sr_407,
    validate_cii_sr_408,
    validate_cii_sr_409,
    validate_cii_sr_421,
    validate_cii_sr_422,
    validate_cii_sr_423,
    validate_cii_sr_424,
    validate_cii_sr_425,
    validate_cii_sr_426,
    validate_cii_sr_427,
    validate_cii_sr_428,
    validate_cii_sr_429,
    validate_cii_sr_430,
    validate_cii_sr_431,
    validate_cii_sr_432,
    validate_cii_sr_433,
    validate_cii_sr_434,
    validate_cii_sr_435,
    validate_cii_sr_436,
    validate_cii_sr_437,
    validate_cii_sr_452,
    validate_cii_sr_453,
    validate_cii_sr_461,
    validate_cii_sr_462,
    validate_cii_sr_411,
    validate_cii_sr_412,
    validate_cii_sr_413,
    validate_cii_sr_414,
    validate_cii_sr_415,
    validate_cii_sr_416,
    validate_cii_sr_417,
    validate_cii_sr_418,
    validate_cii_sr_419,
    validate_cii_sr_420,
    validate_cii_dt_013,
    validate_cii_dt_014,
    validate_cii_sr_438,
    validate_cii_sr_04,
    validate_cii_sr_05,
    validate_cii_dt_001,
    validate_cii_dt_002,
    validate_cii_dt_003,
    validate_cii_dt_004,
    validate_cii_dt_005,
    validate_cii_dt_006,
    validate_cii_dt_007,
    validate_cii_dt_0010,
    validate_cii_dt_0020,
    validate_cii_dt_0030,
    validate_cii_dt_0040,
    validate_cii_dt_008,
    validate_cii_dt_009,
    validate_cii_dt_010,
    validate_cii_dt_011,
    validate_cii_dt_012,
    validate_cii_dt_045,
    validate_cii_dt_046,
    validate_cii_dt_047,
    validate_cii_dt_048,
    validate_cii_dt_015,
    validate_cii_dt_016,
    validate_cii_dt_017,
    validate_cii_dt_018,
    validate_cii_dt_019,
    validate_cii_dt_020,
    validate_cii_dt_021,
    validate_cii_dt_022,
    validate_cii_dt_023,
    validate_cii_dt_024,
    validate_cii_dt_025,
    validate_cii_dt_026,
    validate_cii_dt_027,
    validate_cii_dt_028,
    validate_cii_dt_029,
    validate_cii_dt_030,
    validate_cii_dt_031,
    validate_cii_dt_032,
    validate_cii_dt_033,
    validate_cii_dt_034,
    validate_cii_dt_035,
    validate_cii_dt_036,
    validate_cii_dt_037,
    validate_cii_dt_038,
    validate_cii_dt_039,
    validate_cii_dt_040,
    validate_cii_dt_041,
    validate_cii_dt_042,
    validate_cii_dt_043,
    validate_cii_dt_044,
    validate_cii_dt_049,
    validate_cii_dt_050,
    validate_cii_dt_051,
    validate_cii_dt_052,
    validate_cii_dt_098,
    validate_cii_dt_053,
    validate_cii_dt_054,
    validate_cii_dt_055,
    validate_cii_dt_056,
    validate_cii_dt_057,
    validate_cii_dt_058,
    validate_cii_dt_059,
    validate_cii_dt_060,
    validate_cii_dt_061,
    validate_cii_dt_062,
    validate_cii_dt_063,
    validate_cii_dt_064,
    validate_cii_dt_065,
    validate_cii_dt_066,
    validate_cii_dt_067,
    validate_cii_dt_068,
    validate_cii_dt_069,
    validate_cii_dt_070,
    validate_cii_dt_071,
    validate_cii_dt_072,
    validate_cii_dt_073,
    validate_cii_dt_074,
    validate_cii_dt_075,
    validate_cii_dt_076,
    validate_cii_dt_077,
    validate_cii_dt_078,
    validate_cii_dt_079,
    validate_cii_dt_080,
    validate_cii_dt_081,
    validate_cii_dt_082,
    validate_cii_dt_083,
    validate_cii_dt_084,
    validate_cii_dt_086,
    validate_cii_dt_087,
    validate_cii_dt_088,
    validate_cii_dt_089,
    validate_cii_dt_090,
    validate_cii_dt_091,
    validate_cii_dt_092,
    validate_cii_dt_093,
    validate_cii_dt_094,
    validate_cii_dt_095,
    validate_cii_dt_096,
    validate_cii_dt_097,
    validate_br_cl_01,
    validate_br_cl_03,
    validate_br_cl_04,
    validate_br_cl_05,
    validate_br_cl_06,
    validate_br_cl_07,
    validate_br_cl_08,
    validate_br_cl_10,
    validate_br_cl_11,
    validate_br_cl_13,
    validate_br_cl_14,
    validate_br_cl_15,
    validate_br_cl_16,
    validate_br_cl_17,
    validate_br_cl_18,
    validate_br_cl_19,
    validate_br_cl_20,
    validate_br_cl_21,
    validate_br_cl_22,
    validate_br_cl_23,
    validate_br_cl_24,
    validate_br_cl_25,
    validate_br_cl_26,
];
// Context: //ram:SpecifiedTradeAllowanceCharge
// Test: (ram:ChargeIndicator)
fn validate_br_66(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-66",
            "[BR-66]-Each Specified Trade Allowance Charge (BG-20)(BG-21) shall contain a Charge Indicator.",
        )));
    }
    Ok(())
}

// Context: //ram:DesignatedProductClassification
// Test: normalize-space(ram:ClassCode/@listID) != '' or not (ram:ClassCode)
fn validate_br_65(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-65",
            "[BR-65]-The Item classification identifier (BT-158) shall have a Scheme identifier.",
        )));
    }
    Ok(())
}

// Context: //ram:AdditionalReferencedDocument
// Test: normalize-space(ram:IssuerAssignedID) != ''
fn validate_br_52(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-52",
            "[BR-52]-Each Additional supporting document (BG-24) shall contain a Supporting document reference (BT-122).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableTradeSettlementFinancialCard
// Test: string-length(ram:ID)<=10
fn validate_br_51(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-51",
            "[BR-51]-In accordance with card payments security standards an invoice should never include a full card primary account number (BT-97). At the moment PCI Security Standards Council has defined that the first 6 digits and last 4 digits are the maximum number of digits to be shown.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementPaymentMeans[ram:TypeCode='30' or ram:TypeCode='58']/ram:PayerPartyDebtorFinancialAccount
// Test: (ram:IBANID) or (ram:ProprietaryID)
fn validate_br_50(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-50",
            "[BR-50]-A Payment account identifier (BT-84) shall be present if Credit transfer (BG-16) information is provided in the Invoice.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementPaymentMeans[ram:TypeCode='30' or ram:TypeCode='58']/ram:PayerPartyDebtorFinancialAccount
// Test: (ram:IBANID) or (ram:ProprietaryID)
fn validate_br_61(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-61",
            "[BR-61]-If the Payment means type code (BT-81) means SEPA credit transfer, Local credit transfer or Non-SEPA international credit transfer, the Payment account identifier (BT-84) shall be present.",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: (ram:ShipToTradeParty/ram:PostalTradeAddress and normalize-space(ram:ShipToTradeParty/ram:PostalTradeAddress/ram:CountryID) != '') or not (ram:ShipToTradeParty/ram:PostalTradeAddress)
fn validate_br_57(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-57",
            "[BR-57]-Each Deliver to address (BG-15) shall contain a Deliver to country code (BT-80).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='false']
// Test: (../ram:ActualAmount)
fn validate_br_31(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-31",
            "[BR-31]-Each Document level allowance (BG-20) shall have a Document level allowance amount (BT-92).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='false']
// Test: (../ram:CategoryTradeTax[upper-case(ram:TypeCode) = 'VAT']/ram:CategoryCode)
fn validate_br_32(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-32",
            "[BR-32]-Each Document level allowance (BG-20) shall have a Document level allowance VAT category code (BT-95).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='false']
// Test: (../ram:Reason) or (../ram:ReasonCode)
fn validate_br_33(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-33",
            "[BR-33]-Each Document level allowance (BG-20) shall have a Document level allowance reason (BT-97) or a Document level allowance reason code (BT-98).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='false']
// Test: true()
fn validate_br_co_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-05",
            "[BR-CO-05]-Document level allowance reason code (BT-98) and Document level allowance reason (BT-97) shall indicate the same type of allowance.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='false']
// Test: (../ram:Reason) or (../ram:ReasonCode)
fn validate_br_co_21(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-21",
            "[BR-CO-21]-Each Document level allowance (BG-20) shall contain a Document level allowance reason (BT-97) or a Document level allowance reason code (BT-98), or both.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='false']
// Test: string-length(substring-after(../ram:ActualAmount,'.'))<=2
fn validate_br_dec_01(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-01",
            "[BR-DEC-01]-The allowed maximum number of decimals for the Document level allowance amount (BT-92) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='false']
// Test: string-length(substring-after(../ram:BasisAmount,'.'))<=2
fn validate_br_dec_02(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-02",
            "[BR-DEC-02]-The allowed maximum number of decimals for the Document level allowance base amount (BT-93) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='true']
// Test: (../ram:ActualAmount)
fn validate_br_36(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-36",
            "[BR-36]-Each Document level charge (BG-21) shall have a Document level charge amount (BT-99).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='true']
// Test: (../ram:CategoryTradeTax[upper-case(ram:TypeCode) = 'VAT']/ram:CategoryCode)
fn validate_br_37(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-37",
            "[BR-37]-Each Document level charge (BG-21) shall have a Document level charge VAT category code (BT-102).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='true']
// Test: (../ram:Reason) or (../ram:ReasonCode)
fn validate_br_38(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-38",
            "[BR-38]-Each Document level charge (BG-21) shall have a Document level charge reason (BT-104) or a Document level charge reason code (BT-105).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='true']
// Test: true()
fn validate_br_co_06(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-06",
            "[BR-CO-06]-Document level charge reason code (BT-105) and Document level charge reason (BT-104) shall indicate the same type of charge.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='true']
// Test: (../ram:Reason) or (../ram:ReasonCode)
fn validate_br_co_22(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-22",
            "[BR-CO-22]-Each Document level charge (BG-21) shall contain a Document level charge reason (BT-104) or a Document level charge reason code (BT-105), or both.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='true']
// Test: string-length(substring-after(../ram:ActualAmount,'.'))<=2
fn validate_br_dec_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-05",
            "[BR-DEC-05]-The allowed maximum number of decimals for the Document level charge amount (BT-99) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator='true']
// Test: string-length(substring-after(../ram:BasisAmount,'.'))<=2
fn validate_br_dec_06(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-06",
            "[BR-DEC-06]-The allowed maximum number of decimals for the Document level charge base amount (BT-100) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: (ram:LineTotalAmount)
fn validate_br_12(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-12",
            "[BR-12]-An Invoice shall have the Sum of Invoice line net amount (BT-106).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: (ram:TaxBasisTotalAmount)
fn validate_br_13(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-13",
            "[BR-13]-An Invoice shall have the Invoice total amount without VAT (BT-109).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: (ram:GrandTotalAmount)
fn validate_br_14(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-14",
            "[BR-14]-An Invoice shall have the Invoice total amount with VAT (BT-112).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: (ram:DuePayableAmount)
fn validate_br_15(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-15",
            "[BR-15]-An Invoice shall have the Amount due for payment (BT-115).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: not(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:TaxCurrencyCode) or (/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:TaxCurrencyCode and (ram:TaxTotalAmount/@currencyID = /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:TaxCurrencyCode) and not(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:TaxCurrencyCode = /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:InvoiceCurrencyCode))
fn validate_br_53(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-53",
            "[BR-53]-If the VAT accounting currency code (BT-6) is present, then the Invoice total VAT amount in accounting currency (BT-111) shall be provided.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: xs:decimal(ram:LineTotalAmount) = round(xs:decimal(sum(../../ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeSettlementLineMonetarySummation/ram:LineTotalAmount)) * xs:decimal(100)) div xs:decimal(100)
fn validate_br_co_10(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-10",
            "[BR-CO-10]-Sum of Invoice line net amount (BT-106) = Σ Invoice line net amount (BT-131).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: (not(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()])and not (ram:AllowanceTotalAmount)) or ram:AllowanceTotalAmount = (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:ActualAmount)* 10 * 10 ) div 100)
fn validate_br_co_11(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-11",
            "[BR-CO-11]-Sum of allowances on document level (BT-107) = Σ Document level allowance amount (BT-92).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: (not(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()])and not (ram:ChargeTotalAmount)) or ram:ChargeTotalAmount = (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:ActualAmount)* 10 * 10 ) div 100)
fn validate_br_co_12(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-12",
            "[BR-CO-12]-Sum of charges on document level (BT-108) = Σ Document level charge amount (BT-99).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: (xs:decimal(ram:TaxBasisTotalAmount) = round((xs:decimal(ram:LineTotalAmount) - xs:decimal(ram:AllowanceTotalAmount) + xs:decimal(ram:ChargeTotalAmount)) *10 * 10) div 100) or      ((xs:decimal(ram:TaxBasisTotalAmount) = round((xs:decimal(ram:LineTotalAmount) - xs:decimal(ram:AllowanceTotalAmount)) *10 * 10) div 100)  and not (ram:ChargeTotalAmount)) or      ((xs:decimal(ram:TaxBasisTotalAmount) = round((xs:decimal(ram:LineTotalAmount) + xs:decimal(ram:ChargeTotalAmount)) *10 * 10) div 100)  and not (ram:AllowanceTotalAmount)) or      ((xs:decimal(ram:TaxBasisTotalAmount) = round((xs:decimal(ram:LineTotalAmount))  *10 * 10) div 100) and not (ram:ChargeTotalAmount) and not (ram:AllowanceTotalAmount))
fn validate_br_co_13(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-13",
            "[BR-CO-13]-Invoice total amount without VAT (BT-109) = Σ Invoice line net amount (BT-131) - Sum of allowances on document level (BT-107) + Sum of charges on document level (BT-108).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: (xs:decimal(ram:DuePayableAmount) = xs:decimal(ram:GrandTotalAmount) - xs:decimal(ram:TotalPrepaidAmount) + xs:decimal(ram:RoundingAmount)) or      ((xs:decimal(ram:DuePayableAmount) = xs:decimal(ram:GrandTotalAmount) + xs:decimal(ram:RoundingAmount)) and not (xs:decimal(ram:TotalPrepaidAmount))) or      ((xs:decimal(ram:DuePayableAmount) = xs:decimal(ram:GrandTotalAmount) - xs:decimal(ram:TotalPrepaidAmount)) and not (xs:decimal(ram:RoundingAmount))) or      ((xs:decimal(ram:DuePayableAmount) = xs:decimal(ram:GrandTotalAmount)) and not (xs:decimal(ram:TotalPrepaidAmount)) and not (xs:decimal(ram:RoundingAmount)))
fn validate_br_co_16(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-16",
            "[BR-CO-16]-Amount due for payment (BT-115) = Invoice total amount with VAT (BT-112) -Paid amount (BT-113) +Rounding amount (BT-114).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: string-length(substring-after(ram:LineTotalAmount,'.'))<=2
fn validate_br_dec_09(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-09",
            "[BR-DEC-09]-The allowed maximum number of decimals for the Sum of Invoice line net amount (BT-106) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: string-length(substring-after(ram:AllowanceTotalAmount,'.'))<=2
fn validate_br_dec_10(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-10",
            "[BR-DEC-10]-The allowed maximum number of decimals for the Sum of allowanced on document level (BT-107) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: string-length(substring-after(ram:ChargeTotalAmount,'.'))<=2
fn validate_br_dec_11(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-11",
            "[BR-DEC-11]-The allowed maximum number of decimals for the Sum of charges on document level (BT-108) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: string-length(substring-after(ram:TaxBasisTotalAmount,'.'))<=2
fn validate_br_dec_12(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-12",
            "[BR-DEC-12]-The allowed maximum number of decimals for the Invoice total amount without VAT (BT-109) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: string-length(substring-after(ram:GrandTotalAmount,'.'))<=2
fn validate_br_dec_14(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-14",
            "[BR-DEC-14]-The allowed maximum number of decimals for the Invoice total amount with VAT (BT-112) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: not(ram:TaxTotalAmount) or ram:TaxTotalAmount[(@currencyID =/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:InvoiceCurrencyCode and . = round(. * 100) div 100) or not (@currencyID =/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:InvoiceCurrencyCode)]
fn validate_br_dec_13(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-13",
            "[BR-DEC-13]-The allowed maximum number of decimals for the Invoice total VAT amount (BT-110) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: not(ram:TaxTotalAmount) or ram:TaxTotalAmount[(@currencyID =/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:TaxCurrencyCode and . = round(. * 100) div 100) or not (/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:TaxCurrencyCode)]
fn validate_br_dec_15(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-15",
            "[BR-DEC-15]-The allowed maximum number of decimals for the Invoice total VAT amount in accounting currency (BT-111) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: string-length(substring-after(ram:TotalPrepaidAmount,'.'))<=2
fn validate_br_dec_16(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-16",
            "[BR-DEC-16]-The allowed maximum number of decimals for the Paid amount (BT-113) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: string-length(substring-after(ram:RoundingAmount,'.'))<=2
fn validate_br_dec_17(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-17",
            "[BR-DEC-17]-The allowed maximum number of decimals for the Rounding amount (BT-114) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: string-length(substring-after(ram:DuePayableAmount,'.'))<=2
fn validate_br_dec_18(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-18",
            "[BR-DEC-18]-The allowed maximum number of decimals for the Amount due for payment (BT-115) is 2.",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: normalize-space(rsm:ExchangedDocumentContext/ram:GuidelineSpecifiedDocumentContextParameter/ram:ID) != ''
fn validate_br_01(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-01",
            "[BR-01]-An Invoice shall have a Specification identifier (BT-24).",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: normalize-space(rsm:ExchangedDocument/ram:ID) != ''
fn validate_br_02(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-02",
            "[BR-02]-An Invoice shall have an Invoice number (BT-1).",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: normalize-space(rsm:ExchangedDocument/ram:IssueDateTime/udt:DateTimeString[@format='102']) != ''
fn validate_br_03(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-03",
            "[BR-03]-An Invoice shall have an Invoice issue date (BT-2).",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: normalize-space(rsm:ExchangedDocument/ram:TypeCode) != ''
fn validate_br_04(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-04",
            "[BR-04]-An Invoice shall have an Invoice type code (BT-3).",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: normalize-space(rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:InvoiceCurrencyCode) != ''
fn validate_br_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-05",
            "[BR-05]-An Invoice shall have an Invoice currency code (BT-5).",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: normalize-space(rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:Name) != ''
fn validate_br_06(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-06",
            "[BR-06]-An Invoice shall contain the Seller name (BT-27).",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: normalize-space(rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:BuyerTradeParty/ram:Name) != ''
fn validate_br_07(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-07",
            "[BR-07]-An Invoice shall contain the Buyer name (BT-44).",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:PostalTradeAddress
fn validate_br_08(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-08",
            "[BR-08]-An Invoice shall contain the Seller postal address (BG-5).",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: normalize-space(rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:PostalTradeAddress/ram:CountryID) != ''
fn validate_br_09(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-09",
            "[BR-09]-The Seller postal address (BG-5) shall contain a Seller country code (BT-40).",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:BuyerTradeParty/ram:PostalTradeAddress
fn validate_br_10(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-10",
            "[BR-10]-An Invoice shall contain the Buyer postal address (BG-8).",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: normalize-space(rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:BuyerTradeParty/ram:PostalTradeAddress/ram:CountryID) != ''
fn validate_br_11(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-11",
            "[BR-11]-The Buyer postal address shall contain a Buyer country code (BT-55).",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: //ram:IncludedSupplyChainTradeLineItem
fn validate_br_16(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-16",
            "[BR-16]-An Invoice shall have at least one Invoice line (BG-25).",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: normalize-space(rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:URIUniversalCommunication[1]/ram:URIID/@schemeID) != '' or not (rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:URIUniversalCommunication)
fn validate_br_62(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-62",
            "[BR-62]-The Seller electronic address (BT-34) shall have a Scheme identifier.",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: normalize-space(rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:BuyerTradeParty/ram:URIUniversalCommunication[1]/ram:URIID/@schemeID) != '' or not (rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:BuyerTradeParty/ram:URIUniversalCommunication)
fn validate_br_63(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-63",
            "[BR-63]-The Buyer electronic address (BT-49) shall have a Scheme identifier.",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: every $Currency                                  in rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:InvoiceCurrencyCode                                 satisfies (                                     count ( rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeSettlementHeaderMonetarySummation/ram:TaxTotalAmount[@currencyID=$Currency] ) eq 1 and                                     (//ram:SpecifiedTradeSettlementHeaderMonetarySummation/xs:decimal(ram:GrandTotalAmount) = round(                                      (//ram:SpecifiedTradeSettlementHeaderMonetarySummation/xs:decimal(ram:TaxBasisTotalAmount) +                                      (//ram:SpecifiedTradeSettlementHeaderMonetarySummation/xs:decimal(ram:TaxTotalAmount[@currencyID=$Currency]))) * 10 * 10) div 100)) or                                 (//ram:SpecifiedTradeSettlementHeaderMonetarySummation/xs:decimal(ram:GrandTotalAmount) = (//ram:SpecifiedTradeSettlementHeaderMonetarySummation/xs:decimal(ram:TaxBasisTotalAmount)))
fn validate_br_co_15(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-15",
            "[BR-CO-15]-Invoice total amount with VAT (BT-112) = Invoice total amount without VAT (BT-109) + Invoice total VAT amount (BT-110).",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: (number(//ram:DuePayableAmount) > 0 and ((//ram:SpecifiedTradePaymentTerms/ram:DueDateDateTime) or (//ram:SpecifiedTradePaymentTerms/ram:Description))) or not(number(//ram:DuePayableAmount)>0)
fn validate_br_co_25(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-25",
            "[BR-CO-25]-In case the Amount due for payment (BT-115) is positive, either the Payment due date (BT-9) or the Payment terms (BT-20) shall be present.",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: ((count(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='S']) + count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='S'])) >=2 or not (//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='S'])) and      ((count(//ram:CategoryTradeTax[ram:CategoryCode='S']) + count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='S'])) >=2 or not (//ram:CategoryTradeTax[ram:CategoryCode='S']))
fn validate_br_s_01(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-01",
            "[BR-S-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Standard rated\" shall contain in the VAT breakdown (BG-23) at least one VAT category code (BT-118) equal with \"Standard rated\".",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: (count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='Z'])=0 and count(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='Z'])=0 and count(//ram:CategoryTradeTax[ram:CategoryCode='Z'])=0) or ( count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='Z'])=1 and (exists(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='Z']) or exists(//ram:CategoryTradeTax[ram:CategoryCode='Z'])))
fn validate_br_z_01(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-01",
            "[BR-Z-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Zero rated\" shall contain in the VAT breakdown (BG-23) exactly one VAT category code (BT-118) equal with \"Zero rated\".",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: (count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='E'])=0 and count(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='E'])=0 and count(//ram:CategoryTradeTax[ram:CategoryCode='E'])=0) or ( count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='E'])=1 and (exists(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='E']) or exists(//ram:CategoryTradeTax[ram:CategoryCode='E'])))
fn validate_br_e_01(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-01",
            "[BR-E-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Exempt from VAT\" shall contain exactly one VAT breakdown (BG-23) with the VAT category code (BT-118) equal to \"Exempt from VAT\".",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: (count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='AE'])=0 and count(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='AE'])=0 and count(//ram:CategoryTradeTax[ram:CategoryCode='AE'])=0) or ( count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='AE'])=1 and (exists(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='AE']) or exists(//ram:CategoryTradeTax[ram:CategoryCode='AE'])))
fn validate_br_ae_01(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-01",
            "[BR-AE-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Reverse charge\" shall contain in the VAT breakdown (BG-23) exactly one VAT category code (BT-118) equal with \"VAT reverse charge\".",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: (count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='K'])=0 and count(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='K'])=0 and count(//ram:CategoryTradeTax[ram:CategoryCode='K'])=0) or ( count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='K'])=1 and (exists(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='K']) or exists(//ram:CategoryTradeTax[ram:CategoryCode='K'])))
fn validate_br_ic_01(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-01",
            "[BR-IC-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Intra-community supply\" shall contain in the VAT breakdown (BG-23) exactly one VAT category code (BT-118) equal with \"Intra-community supply\".",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: (count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='G'])=0 and count(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='G'])=0 and count(//ram:CategoryTradeTax[ram:CategoryCode='G'])=0) or ( count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='G'])=1 and (exists(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='G']) or exists(//ram:CategoryTradeTax[ram:CategoryCode='G'])))
fn validate_br_g_01(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-01",
            "[BR-G-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Export outside the EU\" shall contain in the VAT breakdown (BG-23) exactly one VAT category code (BT-118) equal with \"Export outside the EU\".",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: not(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='O']) or (      count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='O'])=1 and      (exists(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='O']) or     exists(//ram:CategoryTradeTax[ram:CategoryCode='O'])))
fn validate_br_o_01(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-01",
            "[BR-O-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Not subject to VAT\" shall contain exactly one VAT breakdown group (BG-23) with the VAT category code (BT-118) equal to \"Not subject to VAT\".",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: ((count(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='L']) + count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='L'])) >=2 or not (//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='L'])) and      ((count(//ram:CategoryTradeTax[ram:CategoryCode='L']) + count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='L'])) >=2 or not (//ram:CategoryTradeTax[ram:CategoryCode='L']))
fn validate_br_af_01(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AF-01",
            "[BR-AF-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"IGIC\" shall contain in the VAT breakdown (BG-23) at least one VAT category code (BT-118) equal with \"IGIC\".",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: ((count(//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='M']) + count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='M'])) >=2 or not (//ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='M'])) and      ((count(//ram:CategoryTradeTax[ram:CategoryCode='M']) + count(//ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode='M'])) >=2 or not (//ram:CategoryTradeTax[ram:CategoryCode='M']))
fn validate_br_ag_01(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AG-01",
            "[BR-AG-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"IPSI\" shall contain in the VAT breakdown (BG-23) at least one VAT category code (BT-118) equal with \"IPSI\".",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: (not(//ram:CountryID != 'IT') and //ram:CategoryCode ='B') or (not(//ram:CategoryCode ='B'))
fn validate_br_b_01(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-B-01",
            "[BR-B-01]-An Invoice where the VAT category code (BT-151, BT-95 or BT-102) is “Split payment” shall be a domestic Italian invoice.",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: (//ram:CategoryCode ='B' and (not(//ram:CategoryCode ='S'))) or (not(//ram:CategoryCode ='B'))
fn validate_br_b_02(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-B-02",
            "[BR-B-02]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is “Split payment\" shall not contain an invoice line (BG-25), a Document level allowance (BG-20) or  a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is “Standard rated”.",
        )));
    }
    Ok(())
}

// Context: //ram:IncludedSupplyChainTradeLineItem
// Test: normalize-space(ram:AssociatedDocumentLineDocument/ram:LineID) != ''
fn validate_br_21(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-21",
            "[BR-21]-Each Invoice line (BG-25) shall have an Invoice line identifier (BT-126).",
        )));
    }
    Ok(())
}

// Context: //ram:IncludedSupplyChainTradeLineItem
// Test: (ram:SpecifiedLineTradeDelivery/ram:BilledQuantity)
fn validate_br_22(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-22",
            "[BR-22]-Each Invoice line (BG-25) shall have an Invoiced quantity (BT-129).",
        )));
    }
    Ok(())
}

// Context: //ram:IncludedSupplyChainTradeLineItem
// Test: (ram:SpecifiedLineTradeDelivery/ram:BilledQuantity/@unitCode)
fn validate_br_23(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-23",
            "[BR-23]-An Invoice line (BG-25) shall have an Invoiced quantity unit of measure code (BT-130).",
        )));
    }
    Ok(())
}

// Context: //ram:IncludedSupplyChainTradeLineItem
// Test: (ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeSettlementLineMonetarySummation/ram:LineTotalAmount)
fn validate_br_24(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-24",
            "[BR-24]-Each Invoice line (BG-25) shall have an Invoice line net amount (BT-131).",
        )));
    }
    Ok(())
}

// Context: //ram:IncludedSupplyChainTradeLineItem
// Test: normalize-space(ram:SpecifiedTradeProduct/ram:Name) != ''
fn validate_br_25(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-25",
            "[BR-25]-Each Invoice line (BG-25) shall contain the Item name (BT-153).",
        )));
    }
    Ok(())
}

// Context: //ram:IncludedSupplyChainTradeLineItem
// Test: (ram:SpecifiedLineTradeAgreement/ram:NetPriceProductTradePrice/ram:ChargeAmount)
fn validate_br_26(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-26",
            "[BR-26]-Each Invoice line (BG-25) shall contain the Item net price (BT-146).",
        )));
    }
    Ok(())
}

// Context: //ram:IncludedSupplyChainTradeLineItem
// Test: (ram:SpecifiedLineTradeAgreement/ram:NetPriceProductTradePrice/ram:ChargeAmount) >= 0
fn validate_br_27(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-27",
            "[BR-27]-The Item net price (BT-146) shall NOT be negative.",
        )));
    }
    Ok(())
}

// Context: //ram:IncludedSupplyChainTradeLineItem
// Test: (ram:SpecifiedLineTradeAgreement/ram:GrossPriceProductTradePrice/ram:ChargeAmount >= 0) or not(ram:SpecifiedLineTradeAgreement/ram:GrossPriceProductTradePrice/ram:ChargeAmount)
fn validate_br_28(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-28",
            "[BR-28]-The Item gross price (BT-148) shall NOT be negative.",
        )));
    }
    Ok(())
}

// Context: //ram:IncludedSupplyChainTradeLineItem
// Test: normalize-space(ram:SpecifiedTradeProduct/ram:GlobalID/@schemeID) != '' or not (ram:SpecifiedTradeProduct/ram:GlobalID)
fn validate_br_64(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-64",
            "[BR-64]-The Item standard identifier (BT-157) shall have a Scheme identifier.",
        )));
    }
    Ok(())
}

// Context: //ram:IncludedSupplyChainTradeLineItem
// Test: (ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[upper-case(ram:TypeCode) = 'VAT']/ram:CategoryCode)
fn validate_br_co_04(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-04",
            "[BR-CO-04]-Each Invoice line (BG-25) shall be categorized with an Invoiced item VAT category code (BT-151).",
        )));
    }
    Ok(())
}

// Context: //ram:IncludedSupplyChainTradeLineItem
// Test: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax
fn validate_br_co_18(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-18",
            "[BR-CO-18]-An Invoice shall at least have one VAT breakdown group (BG-23).",
        )));
    }
    Ok(())
}

// Context: //ram:IncludedSupplyChainTradeLineItem
// Test: string-length(substring-after(ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeSettlementLineMonetarySummation/ram:LineTotalAmount,'.'))<=2
fn validate_br_dec_23(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-23",
            "[BR-DEC-23]-The allowed maximum number of decimals for the Invoice line net amount (BT-131) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator = 'false']
// Test: (../ram:ActualAmount)
fn validate_br_41(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-41",
            "[BR-41]-Each Invoice line allowance (BG-27) shall have an Invoice line allowance amount (BT-136).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator = 'false']
// Test: (../ram:Reason) or (../ram:ReasonCode)
fn validate_br_42(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-42",
            "[BR-42]-Each Invoice line allowance (BG-27) shall have an Invoice line allowance reason (BT-139) or an Invoice line allowance reason code (BT-140).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator = 'false']
// Test: true()
fn validate_br_co_07(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-07",
            "[BR-CO-07]-Invoice line allowance reason code (BT-140) and Invoice line allowance reason (BT-139) shall indicate the same type of allowance reason.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator = 'false']
// Test: (../ram:Reason) or (../ram:ReasonCode)
fn validate_br_co_23(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-23",
            "[BR-CO-23]-Each Invoice line allowance (BG-27) shall contain an Invoice line allowance reason (BT-139) or an Invoice line allowance reason code (BT-140), or both.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator = 'false']
// Test: string-length(substring-after(../ram:ActualAmount,'.'))<=2
fn validate_br_dec_24(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-24",
            "[BR-DEC-24]-The allowed maximum number of decimals for the Invoice line allowance amount (BT-136) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator = 'false']
// Test: string-length(substring-after(../ram:BasisAmount,'.'))<=2
fn validate_br_dec_25(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-25",
            "[BR-DEC-25]-The allowed maximum number of decimals for the Invoice line allowance base amount (BT-137) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator = 'true']
// Test: (../ram:ActualAmount)
fn validate_br_43(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-43",
            "[BR-43]-Each Invoice line charge (BG-28) shall have an Invoice line charge amount (BT-141).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator = 'true']
// Test: (../ram:Reason) or (../ram:ReasonCode)
fn validate_br_44(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-44",
            "[BR-44]-Each Invoice line charge (BG-28) shall have an Invoice line charge reason (BT-144) or an Invoice line charge reason code (BT-145).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator = 'true']
// Test: true()
fn validate_br_co_08(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-08",
            "[BR-CO-08]-Invoice line charge reason code (BT-145) and Invoice line charge reason (BT-144) shall indicate the same type of charge reason.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator = 'true']
// Test: (../ram:Reason) or (../ram:ReasonCode)
fn validate_br_co_24(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-24",
            "[BR-CO-24]-Each Invoice line charge (BG-28) shall contain an Invoice line charge reason (BT-144) or an Invoice line charge reason code (BT-145), or both.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator = 'true']
// Test: string-length(substring-after(../ram:ActualAmount,'.'))<=2
fn validate_br_dec_27(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-27",
            "[BR-DEC-27]-The allowed maximum number of decimals for the Invoice line charge amount (BT-141) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator = 'true']
// Test: string-length(substring-after(../ram:BasisAmount,'.'))<=2
fn validate_br_dec_28(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-28",
            "[BR-DEC-28]-The allowed maximum number of decimals for the Invoice line charge base amount (BT-142) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:BillingSpecifiedPeriod
// Test: (ram:EndDateTime/udt:DateTimeString[@format = '102']) >= (ram:StartDateTime/udt:DateTimeString[@format = '102']) or not (ram:EndDateTime) or not (ram:StartDateTime)
fn validate_br_30(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-30",
            "[BR-30]-If both Invoice line period start date (BT-134) and Invoice line period end date (BT-135) are given then the Invoice line period end date (BT-135) shall be later or equal to the Invoice line period start date (BT-134).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedLineTradeSettlement/ram:BillingSpecifiedPeriod
// Test: (ram:StartDateTime) or (ram:EndDateTime)
fn validate_br_co_20(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-20",
            "[BR-CO-20]-If Invoice line period (BG-26) is used, the Invoice line period start date (BT-134) or the Invoice line period end date (BT-135) shall be filled, or both.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:BillingSpecifiedPeriod
// Test: (ram:EndDateTime/udt:DateTimeString[@format = '102']) >= (ram:StartDateTime/udt:DateTimeString[@format = '102']) or not (ram:EndDateTime) or not (ram:StartDateTime)
fn validate_br_29(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-29",
            "[BR-29]-If both Invoicing period start date (BT-73) and Invoicing period end date (BT-74) are given then the Invoicing period end date (BT-74) shall be later or equal to the Invoicing period start date (BT-73).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:BillingSpecifiedPeriod
// Test: (ram:StartDateTime) or (ram:EndDateTime)
fn validate_br_co_19(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-19",
            "[BR-CO-19]-If Invoicing period (BG-14) is used, the Invoicing period start date (BT-73) or the Invoicing period end date (BT-74) shall be filled, or both.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableProductCharacteristic
// Test: (ram:Description) and (ram:Value)
fn validate_br_54(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-54",
            "[BR-54]-Each Item attribute (BG-32) shall contain an Item attribute name (BT-160) and an Item attribute value (BT-161).",
        )));
    }
    Ok(())
}

// Context: //ram:PayeeTradeParty
// Test: (ram:Name) and (not(ram:Name = ../../ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:Name) and not(ram:ID = ../../ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:ID) and not(ram:SpecifiedLegalOrganization/ram:ID = ../../ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedLegalOrganization/ram:ID))
fn validate_br_17(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-17",
            "[BR-17]-The Payee name (BT-59) shall be provided in the Invoice, if the Payee (BG-10) is different from the Seller (BG-4).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementPaymentMeans
// Test: (ram:TypeCode)
fn validate_br_49(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-49",
            "[BR-49]-A Payment instruction (BG-16) shall specify the Payment means type code (BT-81).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementPaymentMeans
// Test: (ram:PayeePartyCreditorFinancialAccount/ram:IBANID) or (ram:PayeePartyCreditorFinancialAccount/ram:ProprietaryID) or (not(ram:PayeePartyCreditorFinancialAccount/ram:IBANID) and not(ram:PayeePartyCreditorFinancialAccount/ram:ProprietaryID))
fn validate_br_co_27(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-27",
            "[BR-CO-27]- Either the IBAN or a Proprietary ID (BT-84) shall be used.",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:InvoiceReferencedDocument
// Test: normalize-space(ram:IssuerAssignedID) != ''
fn validate_br_55(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-55",
            "[BR-55]-Each Preceding Invoice reference (BG-3) shall contain a Preceding Invoice reference (BT-25).",
        )));
    }
    Ok(())
}

// Context: //ram:SellerTradeParty
// Test: (ram:ID) or (ram:GlobalID) or (ram:SpecifiedLegalOrganization/ram:ID) or (ram:SpecifiedTaxRegistration/ram:ID[@schemeID='VA'])
fn validate_br_co_26(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-26",
            "[BR-CO-26]-In order for the buyer to automatically identify a supplier, the Seller identifier (BT-29), the Seller legal registration identifier (BT-30) and/or the Seller VAT identifier (BT-31) shall be present.",
        )));
    }
    Ok(())
}

// Context: //ram:SellerTaxRepresentativeTradeParty
// Test: (ram:Name)
fn validate_br_18(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-18",
            "[BR-18]-The Seller tax representative name (BT-62) shall be provided in the Invoice, if the Seller (BG-4) has a Seller tax representative party (BG-11).",
        )));
    }
    Ok(())
}

// Context: //ram:SellerTaxRepresentativeTradeParty
// Test: (ram:PostalTradeAddress)
fn validate_br_19(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-19",
            "[BR-19]-The Seller tax representative postal address (BG-12) shall be provided in the Invoice, if the Seller (BG-4) has a Seller tax representative party (BG-11).",
        )));
    }
    Ok(())
}

// Context: //ram:SellerTaxRepresentativeTradeParty
// Test: (ram:PostalTradeAddress/ram:CountryID)
fn validate_br_20(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-20",
            "[BR-20]-The Seller tax representative postal address (BG-12) shall contain a Tax representative country code (BT-69), if the Seller (BG-4) has a Seller tax representative party (BG-11).",
        )));
    }
    Ok(())
}

// Context: //ram:SellerTaxRepresentativeTradeParty
// Test: normalize-space(ram:SpecifiedTaxRegistration/ram:ID[@schemeID='VA']) != ''
fn validate_br_56(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-56",
            "[BR-56]-Each Seller tax representative party (BG-11) shall have a Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeSettlementHeaderMonetarySummation/ram:TaxTotalAmount[@currencyID=/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:InvoiceCurrencyCode]
// Test: . = (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CalculatedAmount)*10*10)div 100)
fn validate_br_co_14(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-14",
            "[BR-CO-14]-Invoice total VAT amount (BT-110) = Σ VAT category tax amount (BT-117).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTaxRegistration/ram:ID[@schemeID='VA']
// Test: contains(' 1A AD AE AF AG AI AL AM AN AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BL BJ BM BN BO BQ BR BS BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH EL ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS XI YE YT ZA ZM ZW ', concat(' ', substring(.,1,2), ' '))
fn validate_br_co_09(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-09",
            "[BR-CO-09]-The Seller VAT identifier (BT-31), the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48) shall have a prefix in accordance with ISO code ISO 3166-1 alpha-2 by which the country of issue may be identified. Nevertheless, Greece may use the prefix ‘EL’.",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[. = 'AE'][upper-case(../ram:TypeCode) = 'VAT']
// Test: (../ram:BasisAmount -1 < (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'AE']/ram:SpecifiedTradeSettlementLineMonetarySummation/ram:LineTotalAmount)*10*10)div 100) + (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true() and ram:CategoryTradeTax/ram:CategoryCode='AE']/ram:ActualAmount)*10*10)div 100) - (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false() and ram:CategoryTradeTax/ram:CategoryCode='AE']/ram:ActualAmount)*10*10)div 100)) and (../ram:BasisAmount +1 > (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'AE']/ram:SpecifiedTradeSettlementLineMonetarySummation/ram:LineTotalAmount)*10*10)div 100) + (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true() and ram:CategoryTradeTax/ram:CategoryCode='AE']/ram:ActualAmount)*10*10)div 100) - (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false() and ram:CategoryTradeTax/ram:CategoryCode='AE']/ram:ActualAmount)*10*10)div 100))
fn validate_br_ae_08(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-08",
            "[BR-AE-08]-In a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Reverse charge\" the VAT category taxable amount (BT-116) shall equal the sum of Invoice line net amounts (BT-131) minus the sum of Document level allowance amounts (BT-92) plus the sum of Document level charge amounts (BT-99) where the VAT category codes (BT-151, BT-95, BT-102) are \"Reverse charge\".",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[. = 'AE'][upper-case(../ram:TypeCode) = 'VAT']
// Test: ../ram:CalculatedAmount = 0
fn validate_br_ae_09(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-09",
            "[BR-AE-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Reverse charge\" shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[. = 'AE'][upper-case(../ram:TypeCode) = 'VAT']
// Test: (../ram:ExemptionReason) or (../ram:ExemptionReasonCode)
fn validate_br_ae_10(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-10",
            "[BR-AE-10]-A VAT Breakdown (BG-23) with VAT Category code (BT-118) \"Reverse charge\" shall have a VAT exemption reason code (BT-121), meaning \"Reverse charge\" or the VAT exemption reason text (BT-120) \"Reverse charge\" (or the equivalent standard text in another language).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'AE'][upper-case(ram:TypeCode) = 'VAT']
// Test: (//ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or //ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']) and (//ram:BuyerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'] or //ram:BuyerTradeParty/ram:SpecifiedLegalOrganization/ram:ID)
fn validate_br_ae_03(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-03",
            "[BR-AE-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Reverse charge\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48) and/or the Buyer legal registration identifier (BT-47).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'AE'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_ae_06(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-06",
            "[BR-AE-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Reverse charge\" the Document level allowance VAT rate (BT-96) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'AE'][upper-case(ram:TypeCode) = 'VAT']
// Test: (//ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or //ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']) and (//ram:BuyerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'] or //ram:BuyerTradeParty/ram:SpecifiedLegalOrganization/ram:ID)
fn validate_br_ae_04(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-04",
            "[BR-AE-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Reverse charge\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48) and/or the Buyer legal registration identifier (BT-47).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'AE'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_ae_07(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-07",
            "[BR-AE-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Reverse charge\" the Document level charge VAT rate (BT-103) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'AE'][upper-case(ram:TypeCode) = 'VAT']
// Test: (//ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or //ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']) and (//ram:BuyerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'] or //ram:BuyerTradeParty/ram:SpecifiedLegalOrganization/ram:ID)
fn validate_br_ae_02(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-02",
            "[BR-AE-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Reverse charge\" shall contain the Seller VAT Identifier (BT-31), the Seller Tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48) and/or the Buyer legal registration identifier (BT-47).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'AE'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_ae_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-05",
            "[BR-AE-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Reverse charge\" the Invoiced item VAT rate (BT-152) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'L'][upper-case(ram:TypeCode) = 'VAT']
// Test: every $rate in ../ram:RateApplicablePercent/xs:decimal(.) satisfies (../ram:BasisAmount = (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'L' and ram:ApplicableTradeTax/xs:decimal(ram:RateApplicablePercent) =$rate]/ram:SpecifiedTradeSettlementLineMonetarySummation/xs:decimal(ram:LineTotalAmount)) * 10 * 10) div 100 + round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true() and ram:CategoryTradeTax/ram:CategoryCode='L' and ram:CategoryTradeTax/xs:decimal(ram:RateApplicablePercent)=$rate]/xs:decimal(ram:ActualAmount)) * 10 * 10) div 100 - round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false() and ram:CategoryTradeTax/ram:CategoryCode='L' and ram:CategoryTradeTax/xs:decimal(ram:RateApplicablePercent)=$rate]/xs:decimal(ram:ActualAmount)) * 10 * 10) div 100))
fn validate_br_af_08(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AF-08",
            "[BR-AF-08]-For each different value of VAT category rate (BT-119) where the VAT category code (BT-118) is \"IGIC\", the VAT category taxable amount (BT-116) in a VAT breakdown (BG-23) shall equal the sum of Invoice line net amounts (BT-131) plus the sum of document level charge amounts (BT-99) minus the sum of document level allowance amounts (BT-92) where the VAT category code (BT-151, BT-102, BT-95) is \"IGIC\" and the VAT rate (BT-152, BT-103, BT-96) equals the VAT category rate (BT-119).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'L'][upper-case(ram:TypeCode) = 'VAT']
// Test: true()
fn validate_br_af_09(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AF-09",
            "[BR-AF-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where VAT category code (BT-118) is \"IGIC\" shall equal the VAT category taxable amount (BT-116) multiplied by the VAT category rate (BT-119).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'L'][upper-case(ram:TypeCode) = 'VAT']
// Test: not(ram:ExemptionReason) and not (ram:ExemptionReasonCode)
fn validate_br_af_10(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AF-10",
            "[BR-AF-10]-A VAT Breakdown (BG-23) with VAT Category code (BT-118) \"IGIC\" shall not have a VAT exemption reason code (BT-121) or VAT exemption reason text (BT-120).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'L'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_af_02(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AF-02",
            "[BR-AF-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"IGIC\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'L'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent > 0
fn validate_br_af_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AF-05",
            "[BR-AF-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"IGIC\" the invoiced item VAT rate (BT-152) shall be greater than 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'L'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_af_03(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AF-03",
            "[BR-AF-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"IGIC\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'L'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent > 0
fn validate_br_af_06(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AF-06",
            "[BR-AF-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"IGIC\" the Document level allowance VAT rate (BT-96) shall be 0 (zero) or greater than zero.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'L'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_af_04(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AF-04",
            "[BR-AF-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"IGIC\" shall contain the Seller VAT Identifier (BT-31), the Seller Tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'L'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent > 0
fn validate_br_af_07(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AF-07",
            "[BR-AF-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"IGIC\" the Document level charge VAT rate (BT-103) shall be 0 (zero) or greater than zero.",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'M'][upper-case(ram:TypeCode) = 'VAT']
// Test: every $rate in ../ram:RateApplicablePercent/xs:decimal(.) satisfies (../ram:BasisAmount = (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'M' and ram:ApplicableTradeTax/xs:decimal(ram:RateApplicablePercent) =$rate]/ram:SpecifiedTradeSettlementLineMonetarySummation/xs:decimal(ram:LineTotalAmount)) * 10 * 10) div 100 + round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true() and ram:CategoryTradeTax/ram:CategoryCode='M' and ram:CategoryTradeTax/xs:decimal(ram:RateApplicablePercent)=$rate]/xs:decimal(ram:ActualAmount)) * 10 * 10) div 100 - round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false() and ram:CategoryTradeTax/ram:CategoryCode='M' and ram:CategoryTradeTax/xs:decimal(ram:RateApplicablePercent)=$rate]/xs:decimal(ram:ActualAmount)) * 10 * 10) div 100))
fn validate_br_ag_08(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AG-08",
            "[BR-AG-08]-For each different value of VAT category rate (BT-119) where the VAT category code (BT-118) is \"IPSI\", the VAT category taxable amount (BT-116) in a VAT breakdown (BG-23) shall equal the sum of Invoice line net amounts (BT-131) plus the sum of document level charge amounts (BT-99) minus the sum of document level allowance amounts (BT-92) where the VAT category code (BT-151, BT-102, BT-95) is \"IPSI\" and the VAT rate (BT-152, BT-103, BT-96) equals the VAT category rate (BT-119).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'M'][upper-case(ram:TypeCode) = 'VAT']
// Test: true()
fn validate_br_ag_09(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AG-09",
            "[BR-AG-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where VAT category code (BT-118) is \"IPSI\" shall equal the VAT category taxable amount (BT-116) multiplied by the VAT category rate (BT-119).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'M'][upper-case(ram:TypeCode) = 'VAT']
// Test: not(ram:ExemptionReason) and not (ram:ExemptionReasonCode)
fn validate_br_ag_10(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AG-10",
            "[BR-AG-10]-A VAT Breakdown (BG-23) with VAT Category code (BT-118) \"IPSI\" shall not have a VAT exemption reason code (BT-121) or VAT exemption reason text (BT-120).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'M'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_ag_02(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AG-02",
            "[BR-AG-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"IPSI\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'M'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent >= 0
fn validate_br_ag_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AG-05",
            "[BR-AG-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"IPSI\" the Invoiced item VAT rate (BT-152) shall be 0 (zero) or greater than zero.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'M'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_ag_03(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AG-03",
            "[BR-AG-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"IPSI\" shall contain the Seller VAT Identifier (BT-31), the Seller Tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'M'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent > 0
fn validate_br_ag_06(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AG-06",
            "[BR-AG-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"IPSI\" the Document level allowance VAT rate (BT-96) shall be 0 (zero) or greater than zero.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'M'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_ag_04(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AG-04",
            "[BR-AG-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"IPSI\" shall contain the Seller VAT Identifier (BT-31), the Seller Tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'M'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent > 0
fn validate_br_ag_07(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AG-07",
            "[BR-AG-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"IPSI\" the Document level charge VAT rate (BT-103) shall be 0 (zero) or greater than zero.",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[. = 'E'][upper-case(../ram:TypeCode) = 'VAT']
// Test: (../ram:BasisAmount - 1 < (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'E']/ram:SpecifiedTradeSettlementLineMonetarySummation/ram:LineTotalAmount)*10*10)div 100) + (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true() and ram:CategoryTradeTax/ram:CategoryCode='E']/ram:ActualAmount)*10*10)div 100) - (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false() and ram:CategoryTradeTax/ram:CategoryCode='E']/ram:ActualAmount)*10*10)div 100)) and (../ram:BasisAmount + 1 > (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'E']/ram:SpecifiedTradeSettlementLineMonetarySummation/ram:LineTotalAmount)*10*10)div 100) + (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true() and ram:CategoryTradeTax/ram:CategoryCode='E']/ram:ActualAmount)*10*10)div 100) - (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false() and ram:CategoryTradeTax/ram:CategoryCode='E']/ram:ActualAmount)*10*10)div 100))
fn validate_br_e_08(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-08",
            "[BR-E-08]-In a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Exempt from VAT\" the VAT category taxable amount (BT-116) shall equal the sum of Invoice line net amounts (BT-131) minus the sum of Document level allowance amounts (BT-92) plus the sum of Document level charge amounts (BT-99) where the VAT category codes (BT-151, BT-95, BT-102) are \"Exempt from VAT\".",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[. = 'E'][upper-case(../ram:TypeCode) = 'VAT']
// Test: ../ram:CalculatedAmount = 0
fn validate_br_e_09(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-09",
            "[BR-E-09]-The VAT category tax amount (BT-117) In a VAT breakdown (BG-23) where the VAT category code (BT-118) equals \"Exempt from VAT\" shall equal 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[. = 'E'][upper-case(../ram:TypeCode) = 'VAT']
// Test: (../ram:ExemptionReason) or (../ram:ExemptionReasonCode)
fn validate_br_e_10(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-10",
            "[BR-E-10]-A VAT Breakdown (BG-23) with VAT Category code (BT-118) \"Exempt from VAT\" shall have a VAT exemption reason code (BT-121) or a VAT exemption reason text (BT-120).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'E'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_e_03(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-03",
            "[BR-E-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Exempt from VAT\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'E'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_e_06(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-06",
            "[BR-E-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Exempt from VAT\", the Document level allowance VAT rate (BT-96) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'E'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_e_04(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-04",
            "[BR-E-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Exempt from VAT\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'E'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_e_07(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-07",
            "[BR-E-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Exempt from VAT\", the Document level charge VAT rate (BT-103) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'E'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_e_02(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-02",
            "[BR-E-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Exempt from VAT\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'E'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_e_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-05",
            "[BR-E-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Exempt from VAT\", the Invoiced item VAT rate (BT-152) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[. = 'G'][upper-case(../ram:TypeCode) = 'VAT']
// Test: (../ram:BasisAmount -1 < (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'G']/ram:SpecifiedTradeSettlementLineMonetarySummation/ram:LineTotalAmount)*10*10)div 100) + (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true() and ram:CategoryTradeTax/ram:CategoryCode='G']/ram:ActualAmount)*10*10)div 100) - (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false() and ram:CategoryTradeTax/ram:CategoryCode='G']/ram:ActualAmount)*10*10)div 100)) and (../ram:BasisAmount +1 > (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'G']/ram:SpecifiedTradeSettlementLineMonetarySummation/ram:LineTotalAmount)*10*10)div 100) + (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true() and ram:CategoryTradeTax/ram:CategoryCode='G']/ram:ActualAmount)*10*10)div 100) - (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false() and ram:CategoryTradeTax/ram:CategoryCode='G']/ram:ActualAmount)*10*10)div 100))
fn validate_br_g_08(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-08",
            "[BR-G-08]-In a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Export outside the EU\" the VAT category taxable amount (BT-116) shall equal the sum of Invoice line net amounts (BT-131) minus the sum of Document level allowance amounts (BT-92) plus the sum of Document level charge amounts (BT-99) where the VAT category codes (BT-151, BT-95, BT-102) are \"Export outside the EU\".",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[. = 'G'][upper-case(../ram:TypeCode) = 'VAT']
// Test: ../ram:CalculatedAmount = 0
fn validate_br_g_09(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-09",
            "[BR-G-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Export outside the EU\" shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[. = 'G'][upper-case(../ram:TypeCode) = 'VAT']
// Test: (../ram:ExemptionReason) or (../ram:ExemptionReasonCode)
fn validate_br_g_10(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-10",
            "[BR-G-10]-A VAT Breakdown (BG-23) with the VAT Category code (BT-118) \"Export outside the EU\" shall have a VAT exemption reason code (BT-121), meaning \"Export outside the EU\" or the VAT exemption reason text (BT-120) \"Export outside the EU\" (or the equivalent standard text in another language).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'G'][upper-case(ram:TypeCode) = 'VAT']
// Test: (//ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'] or //ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'])
fn validate_br_g_03(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-03",
            "[BR-G-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Export outside the EU\" shall contain the Seller VAT Identifier (BT-31) or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'G'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_g_06(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-06",
            "[BR-G-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Export outside the EU\" the Document level allowance VAT rate (BT-96) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'G'][upper-case(ram:TypeCode) = 'VAT']
// Test: (//ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'] or //ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'])
fn validate_br_g_04(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-04",
            "[BR-G-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Export outside the EU\" shall contain the Seller VAT Identifier (BT-31) or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'G'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_g_07(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-07",
            "[BR-G-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Export outside the EU\" the Document level charge VAT rate (BT-103) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'G'][upper-case(ram:TypeCode) = 'VAT']
// Test: (//ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'] or //ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'])
fn validate_br_g_02(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-02",
            "[BR-G-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Export outside the EU\" shall contain the Seller VAT Identifier (BT-31) or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'G'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_g_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-05",
            "[BR-G-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Export outside the EU\" the Invoiced item VAT rate (BT-152) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[.= 'K'][upper-case(../ram:TypeCode) = 'VAT']
// Test: (../ram:BasisAmount - 1 < (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'K']/ram:SpecifiedTradeSettlementLineMonetarySummation/ram:LineTotalAmount)*10*10)div 100) + (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true() and ram:CategoryTradeTax/ram:CategoryCode='K']/ram:ActualAmount)*10*10)div 100) - (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false() and ram:CategoryTradeTax/ram:CategoryCode='K']/ram:ActualAmount)*10*10)div 100)) and (../ram:BasisAmount + 1 > (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'K']/ram:SpecifiedTradeSettlementLineMonetarySummation/ram:LineTotalAmount)*10*10)div 100) + (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true() and ram:CategoryTradeTax/ram:CategoryCode='K']/ram:ActualAmount)*10*10)div 100) - (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false() and ram:CategoryTradeTax/ram:CategoryCode='K']/ram:ActualAmount)*10*10)div 100))
fn validate_br_ic_08(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-08",
            "[BR-IC-08]-In a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Intra-community supply\" the VAT category taxable amount (BT-116) shall equal the sum of Invoice line net amounts (BT-131) minus the sum of Document level allowance amounts (BT-92) plus the sum of Document level charge amounts (BT-99) where the VAT category codes (BT-151, BT-95, BT-102) are \"Intra-community supply\".",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[.= 'K'][upper-case(../ram:TypeCode) = 'VAT']
// Test: ../ram:CalculatedAmount = 0
fn validate_br_ic_09(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-09",
            "[BR-IC-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Intra-community supply\" shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[.= 'K'][upper-case(../ram:TypeCode) = 'VAT']
// Test: (../ram:ExemptionReason) or (../ram:ExemptionReasonCode)
fn validate_br_ic_10(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-10",
            "[BR-IC-10]-A VAT Breakdown (BG-23) with the VAT Category code (BT-118) \"Intra-community supply\" shall have a VAT exemption reason code (BT-121), meaning \"Intra-community supply\" or the VAT exemption reason text (BT-120) \"Intra-community supply\" (or the equivalent standard text in another language).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[.= 'K'][upper-case(../ram:TypeCode) = 'VAT']
// Test: (/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery/ram:ActualDeliverySupplyChainEvent/ram:OccurrenceDateTime/udt:DateTimeString) or (../../ram:BillingSpecifiedPeriod/ram:StartDateTime) or (../../ram:BillingSpecifiedPeriod/ram:EndDateTime)
fn validate_br_ic_11(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-11",
            "[BR-IC-11]-In an Invoice with a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Intra-community supply\" the Actual delivery date (BT-72) or the Invoicing period (BG-14) shall not be blank.",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[.= 'K'][upper-case(../ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery/ram:ShipToTradeParty/ram:PostalTradeAddress/ram:CountryID
fn validate_br_ic_12(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-12",
            "[BR-IC-12]-In an Invoice with a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Intra-community supply\" the Deliver to country code (BT-80) shall not be blank.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'K'][upper-case(ram:TypeCode) = 'VAT']
// Test: (//ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'] or //ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']) and //ram:BuyerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_ic_03(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-03",
            "[BR-IC-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Intra-community supply\" shall contain the Seller VAT Identifier (BT-31) or the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'K'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_ic_06(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-06",
            "[BR-IC-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Intra-community supply\" the Document level allowance VAT rate (BT-96) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'K'][upper-case(ram:TypeCode) = 'VAT']
// Test: (//ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'] or //ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']) and //ram:BuyerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_ic_04(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-04",
            "[BR-IC-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Intra-community supply\" shall contain the Seller VAT Identifier (BT-31) or the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'K'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_ic_07(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-07",
            "[BR-IC-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Intra-community supply\" the Document level charge VAT rate (BT-103) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'K'][upper-case(ram:TypeCode) = 'VAT']
// Test: (//ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'] or //ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']) and //ram:BuyerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_ic_02(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-02",
            "[BR-IC-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Intra-community supply\" shall contain the Seller VAT Identifier (BT-31) or the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'K'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_ic_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-05",
            "[BR-IC-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Intracommunity supply\" the Invoiced item VAT rate (BT-152) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'O'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:BasisAmount = (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'O']/ram:SpecifiedTradeSettlementLineMonetarySummation/xs:decimal(ram:LineTotalAmount))*10*10)div 100) + (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[(ram:ChargeIndicator/udt:Indicator cast as xs:boolean)=true() and ram:CategoryTradeTax/ram:CategoryCode='O']/xs:decimal(ram:ActualAmount))*10*10)div 100) - (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[(ram:ChargeIndicator/udt:Indicator cast as xs:boolean)=false() and ram:CategoryTradeTax/ram:CategoryCode='O']/xs:decimal(ram:ActualAmount))*10*10)div 100)
fn validate_br_o_08(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-08",
            "[BR-O-08]-In a VAT breakdown (BG-23) where the VAT category code (BT-118) is \" Not subject to VAT\" the VAT category taxable amount (BT-116) shall equal the sum of Invoice line net amounts (BT-131) minus the sum of Document level allowance amounts (BT-92) plus the sum of Document level charge amounts (BT-99) where the VAT category codes (BT-151, BT-95, BT-102) are \"Not subject to VAT\".",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'O'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:CalculatedAmount = 0
fn validate_br_o_09(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-09",
            "[BR-O-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Not subject to VAT\" shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'O'][upper-case(ram:TypeCode) = 'VAT']
// Test: (ram:ExemptionReason) or (ram:ExemptionReasonCode)
fn validate_br_o_10(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-10",
            "[BR-O-10]-A VAT Breakdown (BG-23) with VAT Category code (BT-118) \" Not subject to VAT\" shall have a VAT exemption reason code (BT-121), meaning \" Not subject to VAT\" or a VAT exemption reason text (BT-120) \" Not subject to VAT\" (or the equivalent standard text in another language).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'O'][upper-case(ram:TypeCode) = 'VAT']
// Test: not(//ram:ApplicableTradeTax[ram:CategoryCode != 'O'])
fn validate_br_o_11(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-11",
            "[BR-O-11]-An Invoice that contains a VAT breakdown group (BG-23) with a VAT category code (BT-118) \"Not subject to VAT\" shall not contain other VAT breakdown groups (BG-23).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'O'][upper-case(ram:TypeCode) = 'VAT']
// Test: not(//ram:ApplicableTradeTax[ram:CategoryCode != 'O'])
fn validate_br_o_12(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-12",
            "[BR-O-12]-An Invoice that contains a VAT breakdown group (BG-23) with a VAT category code (BT-118) \"Not subject to VAT\" shall not contain an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is not \"Not subject to VAT\".",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'O'][upper-case(ram:TypeCode) = 'VAT']
// Test: not(//ram:CategoryTradeTax[ram:CategoryCode != 'O'])
fn validate_br_o_13(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-13",
            "[BR-O-13]-An Invoice that contains a VAT breakdown group (BG-23) with a VAT category code (BT-118) \"Not subject to VAT\" shall not contain Document level allowances (BG-20) where Document level allowance VAT category code (BT-95) is not \"Not subject to VAT\".",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'O'][upper-case(ram:TypeCode) = 'VAT']
// Test: not(//ram:CategoryTradeTax[ram:CategoryCode != 'O'])
fn validate_br_o_14(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-14",
            "[BR-O-14]-An Invoice that contains a VAT breakdown group (BG-23) with a VAT category code (BT-118) \"Not subject to VAT\" shall not contain Document level charges (BG-21) where Document level charge VAT category code (BT-102) is not \"Not subject to VAT\".",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'O'][upper-case(ram:TypeCode) = 'VAT']
// Test: not(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']) and not (/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']) and not (/ram:BuyerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'])
fn validate_br_o_03(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-03",
            "[BR-O-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Not subject to VAT\" shall not contain the Seller VAT identifier (BT-31), the Seller tax representative VAT identifier (BT-63) or the Buyer VAT identifier (BT-48).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'O'][upper-case(ram:TypeCode) = 'VAT']
// Test: not(ram:RateApplicablePercent)
fn validate_br_o_06(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-06",
            "[BR-O-06]-A Document level allowance (BG-20) where VAT category code (BT-95) is \"Not subject to VAT\" shall not contain a Document level allowance VAT rate (BT-96).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'O'][upper-case(ram:TypeCode) = 'VAT']
// Test: not(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']) and not (/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']) and not (/ram:BuyerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'])
fn validate_br_o_04(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-04",
            "[BR-O-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Not subject to VAT\" shall not contain the Seller VAT identifier (BT-31), the Seller tax representative VAT identifier (BT-63) or the Buyer VAT identifier (BT-48).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'O'][upper-case(ram:TypeCode) = 'VAT']
// Test: not(ram:RateApplicablePercent)
fn validate_br_o_07(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-07",
            "[BR-O-07]-A Document level charge (BG-21) where the VAT category code (BT-102) is \"Not subject to VAT\" shall not contain a Document level charge VAT rate (BT-103).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'O'][upper-case(ram:TypeCode) = 'VAT']
// Test: not(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']) and not (/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']) and not (/ram:BuyerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA'])
fn validate_br_o_02(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-02",
            "[BR-O-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Not subject to VAT\" shall not contain the Seller VAT identifier (BT-31), the Seller tax representative VAT identifier (BT-63) or the Buyer VAT identifier (BT-48).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'O'][upper-case(ram:TypeCode) = 'VAT']
// Test: not(ram:RateApplicablePercent)
fn validate_br_o_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-05",
            "[BR-O-05]-An Invoice line (BG-25) where the VAT category code (BT-151) is \"Not subject to VAT\" shall not contain an Invoiced item VAT rate (BT-152).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[.='S']
// Test: every $rate in ../ram:RateApplicablePercent/xs:decimal(.) satisfies (../ram:BasisAmount = (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'S' and ram:ApplicableTradeTax/xs:decimal(ram:RateApplicablePercent) =$rate]/ram:SpecifiedTradeSettlementLineMonetarySummation/xs:decimal(ram:LineTotalAmount)) * 10 * 10) div 100 + round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true() and ram:CategoryTradeTax/ram:CategoryCode='S' and ram:CategoryTradeTax/xs:decimal(ram:RateApplicablePercent)=$rate]/xs:decimal(ram:ActualAmount)) * 10 * 10) div 100 - round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false() and ram:CategoryTradeTax/ram:CategoryCode='S' and ram:CategoryTradeTax/xs:decimal(ram:RateApplicablePercent)=$rate]/xs:decimal(ram:ActualAmount)) * 10 * 10) div 100))
fn validate_br_s_08(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-08",
            "[BR-S-08]-For each different value of VAT category rate (BT-119) where the VAT category code (BT-118) is \"Standard rated\", the VAT category taxable amount (BT-116) in a VAT breakdown (BG-23) shall equal the sum of Invoice line net amounts (BT-131) plus the sum of document level charge amounts (BT-99) minus the sum of document level allowance amounts (BT-92) where the VAT category code (BT-151, BT-102, BT-95) is \"Standard rated\" and the VAT rate (BT-152, BT-103, BT-96) equals the VAT category rate (BT-119).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[.='S']
// Test: (abs(xs:decimal(../ram:CalculatedAmount)) - 1 < round(abs(xs:decimal(../ram:BasisAmount)) * ../ram:RateApplicablePercent) div 100 ) and (abs(xs:decimal(../ram:CalculatedAmount)) + 1 > round(abs(xs:decimal(../ram:BasisAmount)) * ../ram:RateApplicablePercent) div 100 )
fn validate_br_s_09(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-09",
            "[BR-S-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where VAT category code (BT-118) is \"Standard rated\" shall equal the VAT category taxable amount (BT-116) multiplied by the VAT category rate (BT-119).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[.='S']
// Test: not(../ram:ExemptionReason) and not (../ram:ExemptionReasonCode)
fn validate_br_s_10(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-10",
            "[BR-S-10]-A VAT Breakdown (BG-23) with VAT Category code (BT-118) \"Standard rate\" shall not have a VAT exemption reason code (BT-121) or VAT exemption reason text (BT-120).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'S'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_s_02(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-02",
            "[BR-S-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Standard rated\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'S'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent > 0
fn validate_br_s_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-05",
            "[BR-S-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Standard rated\" the Invoiced item VAT rate (BT-152) shall be greater than zero.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'S'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_s_03(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-03",
            "[BR-S-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Standard rated\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'S'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent > 0
fn validate_br_s_06(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-06",
            "[BR-S-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Standard rated\" the Document level allowance VAT rate (BT-96) shall be greater than zero.",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'S'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_s_04(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-04",
            "[BR-S-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Standard rated\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'S'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent > 0
fn validate_br_s_07(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-07",
            "[BR-S-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Standard rated\" the Document level charge VAT rate (BT-103) shall be greater than zero.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[. = 'Z']
// Test: (../ram:BasisAmount -1 < (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'Z']/ram:SpecifiedTradeSettlementLineMonetarySummation/xs:decimal(ram:LineTotalAmount))*10*10)div 100) + (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[(ram:ChargeIndicator/udt:Indicator cast as xs:boolean)=true() and ram:CategoryTradeTax/ram:CategoryCode='Z']/xs:decimal(ram:ActualAmount))*10*10)div 100) - (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[(ram:ChargeIndicator/udt:Indicator cast as xs:boolean)=false() and ram:CategoryTradeTax/ram:CategoryCode='Z']/xs:decimal(ram:ActualAmount))*10*10)div 100)) and (../ram:BasisAmount +1 > (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement[ram:ApplicableTradeTax/ram:CategoryCode = 'Z']/ram:SpecifiedTradeSettlementLineMonetarySummation/xs:decimal(ram:LineTotalAmount))*10*10)div 100) + (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[(ram:ChargeIndicator/udt:Indicator cast as xs:boolean)=true() and ram:CategoryTradeTax/ram:CategoryCode='Z']/xs:decimal(ram:ActualAmount))*10*10)div 100) - (round(sum(/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeAllowanceCharge[(ram:ChargeIndicator/udt:Indicator cast as xs:boolean)=false() and ram:CategoryTradeTax/ram:CategoryCode='Z']/xs:decimal(ram:ActualAmount))*10*10)div 100))
fn validate_br_z_08(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-08",
            "[BR-Z-08]-In a VAT breakdown (BG-23) where VAT category code (BT-118) is \"Zero rated\" the VAT category taxable amount (BT-116) shall equal the sum of Invoice line net amount (BT-131) minus the sum of Document level allowance amounts (BT-92) plus the sum of Document level charge amounts (BT-99) where the VAT category codes (BT-151, BT-95, BT-102) are \"Zero rated\".",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[. = 'Z']
// Test: ../ram:CalculatedAmount = 0
fn validate_br_z_09(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-09",
            "[BR-Z-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where VAT category code (BT-118) is \"Zero rated\" shall equal 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode[. = 'Z']
// Test: not(../ram:ExemptionReason) and not (../ram:ExemptionReasonCode)
fn validate_br_z_10(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-10",
            "[BR-Z-10]-A VAT Breakdown (BG-23) with VAT Category code (BT-118) \"Zero rated\" shall not have a VAT exemption reason code (BT-121) or VAT exemption reason text (BT-120).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'Z'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_z_03(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-03",
            "[BR-Z-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Zero rated\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=false()]/ram:CategoryTradeTax[ram:CategoryCode = 'Z'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_z_06(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-06",
            "[BR-Z-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Zero rated\" the Document level allowance VAT rate (BT-96) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'Z'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_z_04(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-04",
            "[BR-Z-04]-An Invoice that contains a Document level charge where the Document level charge VAT category code (BT-102) is \"Zero rated\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator=true()]/ram:CategoryTradeTax[ram:CategoryCode = 'Z'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_z_07(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-07",
            "[BR-Z-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Zero rated\" the Document level charge VAT rate (BT-103) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'Z'][upper-case(ram:TypeCode) = 'VAT']
// Test: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = ('VA', 'FC')] or /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:ID[@schemeID = 'VA']
fn validate_br_z_02(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-02",
            "[BR-Z-02]-An Invoice that contains an Invoice line where the Invoiced item VAT category code (BT-151) is \"Zero rated\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: //rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax[ram:CategoryCode = 'Z'][upper-case(ram:TypeCode) = 'VAT']
// Test: ram:RateApplicablePercent = 0
fn validate_br_z_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-05",
            "[BR-Z-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Zero rated\" the Invoiced item VAT rate (BT-152) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax
// Test: (ram:BasisAmount)
fn validate_br_45(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-45",
            "[BR-45]-Each VAT breakdown (BG-23) shall have a VAT category taxable amount (BT-116).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax
// Test: (ram:CalculatedAmount)
fn validate_br_46(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-46",
            "[BR-46]-Each VAT breakdown (BG-23) shall have a VAT category tax amount (BT-117).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax
// Test: (.[upper-case(ram:TypeCode) = 'VAT']/ram:CategoryCode)
fn validate_br_47(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-47",
            "[BR-47]-Each VAT breakdown (BG-23) shall be defined through a VAT category code (BT-118).",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax
// Test: (.[upper-case(ram:TypeCode) = 'VAT']/ram:RateApplicablePercent) or (.[upper-case(ram:TypeCode) = 'VAT']/ram:CategoryCode = 'O')
fn validate_br_48(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-48",
            "[BR-48]-Each VAT breakdown (BG-23) shall have a VAT category rate (BT-119), except if the Invoice is not subject to VAT.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax
// Test: ((//ram:TaxPointDate) and not(//ram:DueDateTypeCode)) or (not (//ram:TaxPointDate) and (//ram:DueDateTypeCode)) or (not (//ram:TaxPointDate) and not (//ram:DueDateTypeCode))
fn validate_br_co_03(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-03",
            "[BR-CO-03]-Value added tax point date (BT-7) and Value added tax point date code (BT-8) are mutually exclusive.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax
// Test: (round(.[normalize-space(upper-case(ram:TypeCode)) = 'VAT']/xs:decimal(ram:RateApplicablePercent)) = 0 and (round(xs:decimal(ram:CalculatedAmount)) = 0)) or (round(.[normalize-space(upper-case(ram:TypeCode)) = 'VAT']/xs:decimal(ram:RateApplicablePercent)) != 0 and ((abs(xs:decimal(ram:CalculatedAmount)) - 1 <= round(abs(xs:decimal(ram:BasisAmount)) * (.[normalize-space(upper-case(ram:TypeCode)) = 'VAT']/xs:decimal(ram:RateApplicablePercent) div 100) * 10 * 10) div 100 ) and (abs(xs:decimal(ram:CalculatedAmount)) + 1 >= round(abs(xs:decimal(ram:BasisAmount)) * (.[normalize-space(upper-case(ram:TypeCode)) = 'VAT']/xs:decimal(ram:RateApplicablePercent) div 100) * 10 * 10) div 100 ))) or (not(exists(.[normalize-space(upper-case(ram:TypeCode))='VAT']/xs:decimal(ram:RateApplicablePercent))) and (round(xs:decimal(ram:CalculatedAmount)) = 0))
fn validate_br_co_17(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-17",
            "[BR-CO-17]-VAT category tax amount (BT-117) = VAT category taxable amount (BT-116) x (VAT category rate (BT-119) / 100), rounded to two decimals.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax
// Test: string-length(substring-after(ram:BasisAmount,'.'))<=2
fn validate_br_dec_19(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-19",
            "[BR-DEC-19]-The allowed maximum number of decimals for the VAT category taxable amount (BT-116) is 2.",
        )));
    }
    Ok(())
}

// Context: //ram:ApplicableHeaderTradeSettlement/ram:ApplicableTradeTax
// Test: string-length(substring-after(ram:CalculatedAmount,'.'))<=2
fn validate_br_dec_20(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-20",
            "[BR-DEC-20]-The allowed maximum number of decimals for the VAT category tax amount (BT-117) is 2.",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext
// Test: not(ram:SpecifiedTransactionID)
fn validate_cii_sr_001(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-001",
            "[CII-SR-001] - SpecifiedTransactionID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext
// Test: not(ram:TestIndicator)
fn validate_cii_sr_002(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-002",
            "[CII-SR-002] - TestIndicator should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext
// Test: (count(ram:BusinessProcessSpecifiedDocumentContextParameter) <= 1)
fn validate_cii_sr_003(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-003",
            "[CII-SR-003] - BusinessProcessSpecifiedDocumentContextParameter should exist maximum once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext
// Test: not(ram:BIMSpecifiedDocumentContextParameter)
fn validate_cii_sr_006(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-006",
            "[CII-SR-006] - BIMSpecifiedDocumentContextParameter should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext
// Test: not(ram:ScenarioSpecifiedDocumentContextParameter)
fn validate_cii_sr_007(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-007",
            "[CII-SR-007] - ScenarioSpecifiedDocumentContextParameter should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext
// Test: not(ram:ApplicationSpecifiedDocumentContextParameter)
fn validate_cii_sr_008(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-008",
            "[CII-SR-008] - ApplicationSpecifiedDocumentContextParameter should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext
// Test: count(ram:GuidelineSpecifiedDocumentContextParameter) = 1
fn validate_cii_sr_009(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-009",
            "[CII-SR-009] - GuidelineSpecifiedDocumentContextParameter must exist exactly once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext
// Test: count(ram:GuidelineSpecifiedDocumentContextParameter/ram:ID) = 1
fn validate_cii_sr_010(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-010",
            "[CII-SR-010] - ID must exist exactly once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext
// Test: not(ram:SubsetSpecifiedDocumentContextParameter)
fn validate_cii_sr_011(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-011",
            "[CII-SR-011] - SubsetSpecifiedDocumentContextParameter should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext
// Test: not(ram:MessageStandardSpecifiedDocumentContextParameter)
fn validate_cii_sr_012(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-012",
            "[CII-SR-012] - MessageStandardSpecifiedDocumentContextParameter should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:Name)
fn validate_cii_sr_013(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-013",
            "[CII-SR-013] - Name should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: count(ram:TypeCode) = 1
fn validate_cii_sr_014(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-014",
            "[CII-SR-014] - TypeCode must exist exactly once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:IssueDateTime/udt:DateTime)
fn validate_cii_sr_015(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-015",
            "[CII-SR-015] - DateTime should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:CopyIndicator)
fn validate_cii_sr_016(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-016",
            "[CII-SR-016] - CopyIndicator should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:Purpose)
fn validate_cii_sr_017(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-017",
            "[CII-SR-017] - Purpose should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:ControlRequirementIndicator)
fn validate_cii_sr_018(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-018",
            "[CII-SR-018] - ControlRequirementIndicator should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:LanguageID)
fn validate_cii_sr_019(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-019",
            "[CII-SR-019] - LanguageID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:PurposeCode)
fn validate_cii_sr_020(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-020",
            "[CII-SR-020] - PurposeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:RevisionDateTime)
fn validate_cii_sr_021(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-021",
            "[CII-SR-021] - RevisionDateTime should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:VersionID)
fn validate_cii_sr_022(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-022",
            "[CII-SR-022] - VersionID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:GlobalID)
fn validate_cii_sr_023(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-023",
            "[CII-SR-023] - GlobalID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:RevisionID)
fn validate_cii_sr_024(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-024",
            "[CII-SR-024] - RevisionID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:PreviousRevisionID)
fn validate_cii_sr_025(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-025",
            "[CII-SR-025] - PreviousRevisionID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:CategoryCode)
fn validate_cii_sr_026(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-026",
            "[CII-SR-026] - CategoryCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:IncludedNote/ram:Subject)
fn validate_cii_sr_027(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-027",
            "[CII-SR-027] - Subject should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:IncludedNote/ram:ContentCode)
fn validate_cii_sr_028(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-028",
            "[CII-SR-028] - ContentCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:IncludedNote/ram:ID)
fn validate_cii_sr_032(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-032",
            "[CII-SR-032] - ID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:EffectiveSpecifiedPeriod)
fn validate_cii_sr_033(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-033",
            "[CII-SR-033] - EffectiveSpecifiedPeriod should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument
// Test: not(ram:IssuerTradeParty)
fn validate_cii_sr_034(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-034",
            "[CII-SR-034] - IssuerTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument/ram:IncludedNote
// Test: count(ram:Content) <= 1
fn validate_cii_sr_030(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-030",
            "[CII-SR-030] - Content should exist maximum once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem
// Test: not(ram:DescriptionCode)
fn validate_cii_sr_035(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-035",
            "[CII-SR-035] - DescriptionCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem
// Test: not(ram:ParentLineID)
fn validate_cii_sr_036(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-036",
            "[CII-SR-036] - ParentLineID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem
// Test: not(ram:LineStatusCode)
fn validate_cii_sr_037(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-037",
            "[CII-SR-037] - LineStatusCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem
// Test: not(ram:LineStatusReasonCode)
fn validate_cii_sr_038(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-038",
            "[CII-SR-038] - LineStatusReasonCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem
// Test: not(ram:IncludedSubordinateTradeLineItem)
fn validate_cii_sr_221(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-221",
            "[CII-SR-221] - IncludedSubordinateTradeLineItem should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:AssociatedDocumentLineDocument
// Test: count(ram:IncludedNote) <= 1
fn validate_cii_sr_039(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-039",
            "[CII-SR-039] - IncludedNote should exist maximum once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:AssociatedDocumentLineDocument
// Test: count(ram:IncludedNote/ram:Content) <= 1
fn validate_cii_sr_040(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-040",
            "[CII-SR-040] - Content should exist maximum once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:AssociatedDocumentLineDocument
// Test: not(ram:IncludedNote/ram:SubjectCode)
fn validate_cii_sr_041(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-041",
            "[CII-SR-041] - SubjectCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:AssociatedDocumentLineDocument
// Test: not(ram:IncludedNote/ram:ID)
fn validate_cii_sr_042(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-042",
            "[CII-SR-042] - ID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:AssociatedDocumentLineDocument
// Test: not(ram:IncludedNote/ram:Subject)
fn validate_cii_sr_043(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-043",
            "[CII-SR-043] - CategoryCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:AssociatedDocumentLineDocument
// Test: not(ram:IncludedNote/ram:ContentCode)
fn validate_cii_sr_044(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-044",
            "[CII-SR-044] - Subject should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ID)
fn validate_cii_sr_045(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-045",
            "[CII-SR-045] - ID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:GlobalID) or (ram:GlobalID/@schemeID)
fn validate_cii_sr_046(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-046",
            "[CII-SR-046] - schemeID must be present if GlobalID is present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ManufacturerAssignedID)
fn validate_cii_sr_048(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-048",
            "[CII-SR-048] - ManufacturerAssignedID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:TradeName)
fn validate_cii_sr_049(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-049",
            "[CII-SR-049] - TradeName should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:TypeCode)
fn validate_cii_sr_050(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-050",
            "[CII-SR-050] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:NetWeightMeasure)
fn validate_cii_sr_051(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-051",
            "[CII-SR-051] - NetWeightMeasure should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:GrossWeightMeasure)
fn validate_cii_sr_052(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-052",
            "[CII-SR-052] - GrossWeightMeasure should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ProductGroupID)
fn validate_cii_sr_053(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-053",
            "[CII-SR-053] - ProductGroupID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:EndItemTypeCode)
fn validate_cii_sr_054(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-054",
            "[CII-SR-054] - EndItemTypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:EndItemName)
fn validate_cii_sr_055(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-055",
            "[CII-SR-055] - EndItemName should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:AreaDensityMeasure)
fn validate_cii_sr_056(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-056",
            "[CII-SR-056] - AreaDensityMeasure should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:UseDescription)
fn validate_cii_sr_057(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-057",
            "[CII-SR-057] - UseDescription should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:BrandName)
fn validate_cii_sr_058(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-058",
            "[CII-SR-058] - BrandName should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:SubBrandName)
fn validate_cii_sr_059(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-059",
            "[CII-SR-059] - SubBrandName should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:DrainedNetWeightMeasure)
fn validate_cii_sr_060(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-060",
            "[CII-SR-060] - DrainedNetWeightMeasure should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:VariableMeasureIndicator)
fn validate_cii_sr_061(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-061",
            "[CII-SR-061] - VariableMeasureIndicator should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ColourCode)
fn validate_cii_sr_062(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-062",
            "[CII-SR-062] - ColourCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ColourDescription)
fn validate_cii_sr_063(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-063",
            "[CII-SR-063] - ColourDescription should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:Designation)
fn validate_cii_sr_064(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-064",
            "[CII-SR-064] - Designation should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:FormattedCancellationAnnouncedLaunchDateTime)
fn validate_cii_sr_065(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-065",
            "[CII-SR-065] - FormattedCancellationAnnouncedLaunchDateTime should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:FormattedLatestProductDataChangeDateTime)
fn validate_cii_sr_066(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-066",
            "[CII-SR-066] - FormattedLatestProductDataChangeDateTime should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ApplicableProductCharacteristic/ram:ID)
fn validate_cii_sr_067(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-067",
            "[CII-SR-067] - ID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ApplicableProductCharacteristic/ram:TypeCode)
fn validate_cii_sr_068(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-068",
            "[CII-SR-068] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ApplicableProductCharacteristic/ram:ValueMeasure)
fn validate_cii_sr_070(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-070",
            "[CII-SR-070] - ValueMeasure should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ApplicableProductCharacteristic/ram:MeasurementMethodCode)
fn validate_cii_sr_071(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-071",
            "[CII-SR-071] - MeasurementMethodCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ApplicableProductCharacteristic/ram:ValueCode)
fn validate_cii_sr_073(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-073",
            "[CII-SR-073] - ValueCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ApplicableProductCharacteristic/ram:ValueDateTime)
fn validate_cii_sr_074(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-074",
            "[CII-SR-074] - ValueDateTime should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ApplicableProductCharacteristic/ram:ValueIndicator)
fn validate_cii_sr_075(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-075",
            "[CII-SR-075] - ValueIndicator should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ApplicableProductCharacteristic/ram:ContentTypeCode)
fn validate_cii_sr_076(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-076",
            "[CII-SR-076] - ContentTypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ApplicableProductCharacteristic/ram:ValueSpecifiedBinaryFile)
fn validate_cii_sr_077(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-077",
            "[CII-SR-077] - ValueSpecifiedBinaryFile should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ApplicableProductCharacteristic/ram:ApplicableProductCharacteristicCondition)
fn validate_cii_sr_078(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-078",
            "[CII-SR-078] - ApplicableProductCharacteristicCondition should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ApplicableProductCharacteristic/ram:ApplicableReferencedStandard)
fn validate_cii_sr_079(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-079",
            "[CII-SR-079] - ApplicableReferencedStandard should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ApplicableMaterialGoodsCharacteristic)
fn validate_cii_sr_080(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-080",
            "[CII-SR-080] - ApplicableMaterialGoodsCharacteristic should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:DesignatedProductClassification/ram:SystemID)
fn validate_cii_sr_081(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-081",
            "[CII-SR-081] - SystemID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:DesignatedProductClassification/ram:SystemName)
fn validate_cii_sr_082(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-082",
            "[CII-SR-082] - SystemName should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:DesignatedProductClassification/ram:ClassName)
fn validate_cii_sr_083(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-083",
            "[CII-SR-083] - ClassName should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:DesignatedProductClassification/ram:SubClassCode)
fn validate_cii_sr_084(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-084",
            "[CII-SR-084] - SubClassCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:DesignatedProductClassification/ram:ClassProductCharacteristic)
fn validate_cii_sr_085(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-085",
            "[CII-SR-085] - ClassProductCharacteristic should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:DesignatedProductClassification/ram:ApplicableReferencedStandard)
fn validate_cii_sr_086(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-086",
            "[CII-SR-086] - ApplicableReferencedStandard should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:IndividualTradeProductInstance)
fn validate_cii_sr_087(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-087",
            "[CII-SR-087] - IndividualTradeProductInstance should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:CertificationEvidenceReferenceReferencedDocument)
fn validate_cii_sr_088(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-088",
            "[CII-SR-088] - CertificationEvidenceReferenceReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:InspectionReferenceReferencedDocument)
fn validate_cii_sr_089(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-089",
            "[CII-SR-089] - InspectionReferenceReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not (ram:OriginTradeCountry) or (count(ram:OriginTradeCountry/ram:ID) =1)
fn validate_cii_sr_090(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-090",
            "[CII-SR-090] - ID should exist maximum once.",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:OriginTradeCountry/ram:Name)
fn validate_cii_sr_091(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-091",
            "[CII-SR-091] - Name should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:OriginTradeCountry/ram:SubordinateTradeCountrySubDivision)
fn validate_cii_sr_092(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-092",
            "[CII-SR-092] - SubordinateTradeCountrySubDivision should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:LinearSpatialDimension)
fn validate_cii_sr_093(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-093",
            "[CII-SR-093] - LinearSpatialDimension should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:MinimumLinearSpatialDimension)
fn validate_cii_sr_094(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-094",
            "[CII-SR-094] - MinimumLinearSpatialDimension should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:MaximumLinearSpatialDimension)
fn validate_cii_sr_095(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-095",
            "[CII-SR-095] - MaximumLinearSpatialDimension should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:ManufacturerTradeParty)
fn validate_cii_sr_096(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-096",
            "[CII-SR-096] - ManufacturerTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:PresentationSpecifiedBinaryFile)
fn validate_cii_sr_097(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-097",
            "[CII-SR-097] - PresentationSpecifiedBinaryFile should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:MSDSReferenceReferencedDocument)
fn validate_cii_sr_098(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-098",
            "[CII-SR-098] - MSDSReferenceReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:AdditionalReferenceReferencedDocument)
fn validate_cii_sr_099(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-099",
            "[CII-SR-099] - AdditionalReferenceReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:LegalRightsOwnerTradeParty)
fn validate_cii_sr_100(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-100",
            "[CII-SR-100] - LegalRightsOwnerTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:BrandOwnerTradeParty)
fn validate_cii_sr_101(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-101",
            "[CII-SR-101] -BrandOwnerTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:IncludedReferencedProduct)
fn validate_cii_sr_102(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-102",
            "[CII-SR-102] -IncludedReferencedProduct should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct
// Test: not(ram:InformationNote)
fn validate_cii_sr_103(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-103",
            "[CII-SR-103] - InformationNoteshould not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct/ram:ApplicableProductCharacteristic
// Test: (count(ram:Description) =1)
fn validate_cii_sr_069(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-069",
            "[CII-SR-069] - Description should exist maximum once.",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct/ram:ApplicableProductCharacteristic
// Test: (count(ram:Value) =1)
fn validate_cii_sr_072(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-072",
            "[CII-SR-072] - Value should exist maximum once.",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:BuyerReference)
fn validate_cii_sr_104(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-104",
            "[CII-SR-104] - BuyerReference should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:BuyerRequisitionerTradeParty)
fn validate_cii_sr_105(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-105",
            "[CII-SR-105] - BuyerRequisitionerTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:ApplicableTradeDeliveryTerms)
fn validate_cii_sr_106(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-106",
            "[CII-SR-106] - ApplicableTradeDeliveryTerms should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:SellerOrderReferencedDocument)
fn validate_cii_sr_107(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-107",
            "[CII-SR-107] - SellerOrderReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:BuyerOrderReferencedDocument/ram:IssuerAssignedID)
fn validate_cii_sr_108(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-108",
            "[CII-SR-108] - IssuerAssignedID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:QuotationReferencedDocument)
fn validate_cii_sr_109(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-109",
            "[CII-SR-109] - QuotationReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:ContractReferencedDocument)
fn validate_cii_sr_110(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-110",
            "[CII-SR-110] - ContractReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:DemandForecastReferencedDocument)
fn validate_cii_sr_111(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-111",
            "[CII-SR-111] - DemandForecastReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:PromotionalDealReferencedDocument)
fn validate_cii_sr_112(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-112",
            "[CII-SR-112] - PromotionalDealReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:AdditionalReferencedDocument)
fn validate_cii_sr_113(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-113",
            "[CII-SR-113] - AdditionalReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:TypeCode)
fn validate_cii_sr_114(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-114",
            "[CII-SR-114] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:MinimumQuantity)
fn validate_cii_sr_115(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-115",
            "[CII-SR-115] - MinimumQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:MaximumQuantity)
fn validate_cii_sr_116(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-116",
            "[CII-SR-116] - MaximumQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:ChangeReason)
fn validate_cii_sr_117(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-117",
            "[CII-SR-117] - ChangeReason should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:OrderUnitConversionFactorNumeric)
fn validate_cii_sr_118(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-118",
            "[CII-SR-118] - OrderUnitConversionFactorNumeric should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: count(ram:NetPriceProductTradePrice/ram:ChargeAmount) = 1
fn validate_cii_sr_439(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-439",
            "[CII-SR-439] - ChargeAmount should exist maximum once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: (ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:ChargeIndicator[udt:Indicator=false()] and ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:ActualAmount) or (not (ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:ChargeIndicator) and not (ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:ActualAmount))
fn validate_cii_sr_119(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-119",
            "[CII-SR-119] - Only allowances on price a price should be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:ID)
fn validate_cii_sr_120(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-120",
            "[CII-SR-120] - ID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:SequenceNumeric)
fn validate_cii_sr_121(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-121",
            "[CII-SR-121] - SequenceNumeric should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:CalculationPercent)
fn validate_cii_sr_122(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-122",
            "[CII-SR-122] - CalculationPercent should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:BasisAmount)
fn validate_cii_sr_123(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-123",
            "[CII-SR-123] - BasisAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:BasisQuantity)
fn validate_cii_sr_124(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-124",
            "[CII-SR-124] - BasisQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:PrepaidIndicator)
fn validate_cii_sr_125(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-125",
            "[CII-SR-125] - PrepaidIndicator should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:UnitBasisAmount)
fn validate_cii_sr_126(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-126",
            "[CII-SR-126] - UnitBasisAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:ReasonCode)
fn validate_cii_sr_127(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-127",
            "[CII-SR-127] - ReasonCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:Reason)
fn validate_cii_sr_128(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-128",
            "[CII-SR-128] - Reason should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:TypeCode)
fn validate_cii_sr_129(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-129",
            "[CII-SR-129] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:CategoryTradeTax)
fn validate_cii_sr_130(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-130",
            "[CII-SR-130] - CategoryTradeTax should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge/ram:ActualTradeCurrencyExchange)
fn validate_cii_sr_131(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-131",
            "[CII-SR-131] - ActualTradeCurrencyExchange should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:IncludedTradeTax)
fn validate_cii_sr_445(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-445",
            "[CII-SR-445] - IncludedTradeTax should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:ValiditySpecifiedPeriod)
fn validate_cii_sr_132(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-132",
            "[CII-SR-132] - ValiditySpecifiedPeriod should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:DeliveryTradeLocation)
fn validate_cii_sr_133(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-133",
            "[CII-SR-133] - DeliveryTradeLocation should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:TradeComparisonReferencePrice)
fn validate_cii_sr_134(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-134",
            "[CII-SR-134] - TradeComparisonReferencePrice should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:GrossPriceProductTradePrice/ram:AssociatedReferencedDocument)
fn validate_cii_sr_135(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-135",
            "[CII-SR-135] - AssociatedReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:NetPriceProductTradePrice/ram:TypeCode)
fn validate_cii_sr_136(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-136",
            "[CII-SR-136] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:NetPriceProductTradePrice/ram:MinimumQuantity)
fn validate_cii_sr_138(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-138",
            "[CII-SR-138] - MinimumQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:NetPriceProductTradePrice/ram:MaximumQuantity)
fn validate_cii_sr_139(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-139",
            "[CII-SR-139] - MaximumQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:NetPriceProductTradePrice/ram:ChangeReason)
fn validate_cii_sr_140(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-140",
            "[CII-SR-140] - ChangeReason should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:NetPriceProductTradePrice/ram:OrderUnitConversionFactorNumeric)
fn validate_cii_sr_141(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-141",
            "[CII-SR-141] - OrderUnitConversionFactorNumeric should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:NetPriceProductTradePrice/ram:AppliedTradeAllowanceCharge)
fn validate_cii_sr_142(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-142",
            "[CII-SR-142] - AppliedTradeAllowanceCharge should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:NetPriceProductTradePrice/ram:IncludedTradeTax)
fn validate_cii_sr_446(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-446",
            "[CII-SR-446] - IncludedTradeTax should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:NetPriceProductTradePrice/ram:ValiditySpecifiedPeriod)
fn validate_cii_sr_143(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-143",
            "[CII-SR-143] - ValiditySpecifiedPeriod should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:NetPriceProductTradePrice/ram:DeliveryTradeLocation)
fn validate_cii_sr_144(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-144",
            "[CII-SR-144] - DeliveryTradeLocation should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:NetPriceProductTradePrice/ram:TradeComparisonReferencePrice)
fn validate_cii_sr_145(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-145",
            "[CII-SR-145] - TradeComparisonReferencePrice should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:NetPriceProductTradePrice/ram:AssociatedReferencedDocument)
fn validate_cii_sr_146(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-146",
            "[CII-SR-146] - AssociatedReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: count(ram:NetPriceProductTradePrice/ram:ChargeAmount) <= 1
fn validate_cii_sr_441(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-441",
            "[CII-SR-441] - ChargeAmount should exist maximum once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:RequisitionerReferencedDocument)
fn validate_cii_sr_147(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-147",
            "[CII-SR-147] - RequisitionerReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:ItemSellerTradeParty)
fn validate_cii_sr_148(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-148",
            "[CII-SR-148] - ItemSellerTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:ItemBuyerTradeParty)
fn validate_cii_sr_149(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-149",
            "[CII-SR-149] - ItemBuyerTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:IncludedSpecifiedMarketplace)
fn validate_cii_sr_150(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-150",
            "[CII-SR-150] - IncludedSpecifiedMarketplace should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeAgreement
// Test: not(ram:UltimateCustomerOrderReferencedDocument)
fn validate_cii_sr_447(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-447",
            "[CII-SR-447] - UltimateCustomerOrderReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:GrossPriceProductTradePrice/ram:AppliedTradeAllowanceCharge
// Test: count(ram:ActualAmount) <= 1
fn validate_cii_sr_440(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-440",
            "[CII-SR-440] - ActualAmount should exist maximum once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:RequestedQuantity)
fn validate_cii_sr_151(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-151",
            "[CII-SR-151] - RequestedQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:ReceivedQuantity)
fn validate_cii_sr_152(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-152",
            "[CII-SR-152] - ReceivedQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:ChargeFreeQuantity)
fn validate_cii_sr_153(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-153",
            "[CII-SR-153] - ChargeFreeQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:PackageQuantity)
fn validate_cii_sr_154(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-154",
            "[CII-SR-154] - PackageQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:ProductUnitQuantity)
fn validate_cii_sr_155(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-155",
            "[CII-SR-155] - ProductUnitQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:PerPackageUnitQuantity)
fn validate_cii_sr_156(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-156",
            "[CII-SR-156] - PerPackageUnitQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:NetWeightMeasure)
fn validate_cii_sr_157(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-157",
            "[CII-SR-157] - NetWeightMeasure should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:GrossWeightMeasure)
fn validate_cii_sr_158(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-158",
            "[CII-SR-158] - GrossWeightMeasure should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:TheoreticalWeightMeasure)
fn validate_cii_sr_159(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-159",
            "[CII-SR-159] - TheoreticalWeightMeasure should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:DespatchedQuantity)
fn validate_cii_sr_160(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-160",
            "[CII-SR-160] - DespatchedQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:SpecifiedDeliveryAdjustment)
fn validate_cii_sr_161(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-161",
            "[CII-SR-161] - SpecifiedDeliveryAdjustment should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:IncludedSupplyChainPackaging)
fn validate_cii_sr_162(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-162",
            "[CII-SR-162] - IncludedSupplyChainPackaging should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:RelatedSupplyChainConsignment)
fn validate_cii_sr_163(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-163",
            "[CII-SR-163] - RelatedSupplyChainConsignment should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:ShipToTradeParty)
fn validate_cii_sr_164(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-164",
            "[CII-SR-164] - ShipToTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:UltimateShipToTradeParty)
fn validate_cii_sr_165(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-165",
            "[CII-SR-165] - UltimateShipToTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:ShipFromTradeParty)
fn validate_cii_sr_166(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-166",
            "[CII-SR-166] - ShipFromTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:ActualDespatchSupplyChainEvent)
fn validate_cii_sr_167(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-167",
            "[CII-SR-167] - ActualDespatchSupplyChainEvent should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:ActualPickUpSupplyChainEvent)
fn validate_cii_sr_168(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-168",
            "[CII-SR-168] - ActualPickUpSupplyChainEvent should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:RequestedDeliverySupplyChainEvent)
fn validate_cii_sr_169(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-169",
            "[CII-SR-169] - RequestedDeliverySupplyChainEvent should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:ActualDeliverySupplyChainEvent)
fn validate_cii_sr_170(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-170",
            "[CII-SR-170] - ActualDeliverySupplyChainEvent should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:ActualReceiptSupplyChainEvent)
fn validate_cii_sr_171(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-171",
            "[CII-SR-171] - ActualReceiptSupplyChainEvent should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:AdditionalReferencedDocument)
fn validate_cii_sr_172(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-172",
            "[CII-SR-172] - AdditionalReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:DespatchAdviceReferencedDocument)
fn validate_cii_sr_173(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-173",
            "[CII-SR-173] - DespatchAdviceReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:ReceivingAdviceReferencedDocument)
fn validate_cii_sr_174(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-174",
            "[CII-SR-174] - ReceivingAdviceReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:DeliveryNoteReferencedDocument)
fn validate_cii_sr_175(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-175",
            "[CII-SR-175] - DeliveryNoteReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:ConsumptionReportReferencedDocument)
fn validate_cii_sr_176(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-176",
            "[CII-SR-176] - ConsumptionReportReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery
// Test: not(ram:PackingListReferencedDocument)
fn validate_cii_sr_177(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-177",
            "[CII-SR-177] - RequestedQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:PaymentReference)
fn validate_cii_sr_178(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-178",
            "[CII-SR-178] - PaymentReference should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:InvoiceIssuerReference)
fn validate_cii_sr_179(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-179",
            "[CII-SR-179] - InvoiceIssuerReference should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:TotalAdjustmentAmount)
fn validate_cii_sr_180(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-180",
            "[CII-SR-180] - TotalAdjustmentAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:DiscountIndicator)
fn validate_cii_sr_181(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-181",
            "[CII-SR-181] - DiscountIndicator should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:ApplicableTradeTax/ram:CalculatedAmount)
fn validate_cii_sr_182(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-182",
            "[CII-SR-182] - CalculatedAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:ChargeIndicator/udt:IndicatorString)
fn validate_cii_sr_183(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-183",
            "[CII-SR-183] - IndicatorString should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:ID)
fn validate_cii_sr_184(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-184",
            "[CII-SR-184] - ID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:SequenceNumeric)
fn validate_cii_sr_185(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-185",
            "[CII-SR-185] - SequenceNumeric should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:CalculationPercent/@format)
fn validate_cii_sr_186(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-186",
            "[CII-SR-186] - @format should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:BasisQuantity)
fn validate_cii_sr_187(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-187",
            "[CII-SR-187] - BasisQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:PrepaidIndicator)
fn validate_cii_sr_188(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-188",
            "[CII-SR-188] - PrepaidIndicator should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:UnitBasisAmount)
fn validate_cii_sr_189(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-189",
            "[CII-SR-189] - UnitBasisAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:TypeCode)
fn validate_cii_sr_190(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-190",
            "[CII-SR-190] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:CategoryTradeTax)
fn validate_cii_sr_191(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-191",
            "[CII-SR-191] - CategoryTradeTax should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:ActualTradeCurrencyExchange)
fn validate_cii_sr_192(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-192",
            "[CII-SR-192] - ActualTradeCurrencyExchange should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:ID)
fn validate_cii_sr_193(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-193",
            "[CII-SR-193] - ID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SubtotalCalculatedTradeTax)
fn validate_cii_sr_194(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-194",
            "[CII-SR-194] - SubtotalCalculatedTradeTax should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedLogisticsServiceCharge)
fn validate_cii_sr_195(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-195",
            "[CII-SR-195] - SpecifiedLogisticsServiceCharge should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradePaymentTerms)
fn validate_cii_sr_196(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-196",
            "[CII-SR-196] - SpecifiedTradePaymentTerms should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementLineMonetarySummation/ram:ChargeTotalAmount)
fn validate_cii_sr_197(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-197",
            "[CII-SR-197] - ChargeTotalAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementLineMonetarySummation/ram:AllowanceTotalAmount)
fn validate_cii_sr_198(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-198",
            "[CII-SR-198] - AllowanceTotalAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementLineMonetarySummation/ram:TaxBasisTotalAmount)
fn validate_cii_sr_199(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-199",
            "[CII-SR-199] - TaxBasisTotalAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementLineMonetarySummation/ram:TaxTotalAmount)
fn validate_cii_sr_200(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-200",
            "[CII-SR-200] - TaxTotalAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementLineMonetarySummation/ram:GrandTotalAmount)
fn validate_cii_sr_201(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-201",
            "[CII-SR-201] - GrandTotalAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementLineMonetarySummation/ram:InformationAmount)
fn validate_cii_sr_202(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-202",
            "[CII-SR-202] - InformationAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementLineMonetarySummation/ram:TotalAllowanceChargeAmount)
fn validate_cii_sr_203(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-203",
            "[CII-SR-203] - TotalAllowanceChargeAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementLineMonetarySummation/ram:TotalRetailValueInformationAmount)
fn validate_cii_sr_204(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-204",
            "[CII-SR-204] - TotalRetailValueInformationAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementLineMonetarySummation/ram:GrossLineTotalAmount)
fn validate_cii_sr_205(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-205",
            "[CII-SR-205] - GrossLineTotalAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementLineMonetarySummation/ram:NetLineTotalAmount)
fn validate_cii_sr_206(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-206",
            "[CII-SR-206] - NetLineTotalAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementLineMonetarySummation/ram:NetIncludingTaxesLineTotalAmount)
fn validate_cii_sr_207(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-207",
            "[CII-SR-207] - NetIncludingTaxesLineTotalAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementLineMonetarySummation/ram:ProductWeightLossInformationAmount)
fn validate_cii_sr_208(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-208",
            "[CII-SR-208] - ProductWeightLossInformationAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedFinancialAdjustment)
fn validate_cii_sr_209(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-209",
            "[CII-SR-209] - SpecifiedFinancialAdjustment should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:InvoiceReferencedDocument)
fn validate_cii_sr_210(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-210",
            "[CII-SR-210] - InvoiceReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:PayableSpecifiedTradeAccountingAccount)
fn validate_cii_sr_212(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-212",
            "[CII-SR-212] - PayableSpecifiedTradeAccountingAccount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:ReceivableSpecifiedTradeAccountingAccount/ram:SetTriggerCode)
fn validate_cii_sr_213(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-213",
            "[CII-SR-213] - SetTriggerCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:ReceivableSpecifiedTradeAccountingAccount/ram:TypeCode)
fn validate_cii_sr_214(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-214",
            "[CII-SR-214] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:ReceivableSpecifiedTradeAccountingAccount/ram:AmountTypeCode)
fn validate_cii_sr_215(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-215",
            "[CII-SR-215] - AmountTypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:ReceivableSpecifiedTradeAccountingAccount/ram:Name)
fn validate_cii_sr_216(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-216",
            "[CII-SR-216] - Name should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:ReceivableSpecifiedTradeAccountingAccount/ram:CostReferenceDimensionPattern)
fn validate_cii_sr_217(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-217",
            "[CII-SR-217] - CostReferenceDimensionPattern should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:PurchaseSpecifiedTradeAccountingAccount)
fn validate_cii_sr_218(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-218",
            "[CII-SR-218] - PurchaseSpecifiedTradeAccountingAccount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SalesSpecifiedTradeAccountingAccount)
fn validate_cii_sr_219(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-219",
            "[CII-SR-219] - SalesSpecifiedTradeAccountingAccount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementFinancialCard)
fn validate_cii_sr_220(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-220",
            "[CII-SR-220] - SpecifiedTradeSettlementFinancialCard should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement
// Test: count(ram:ApplicableTradeTax) = 1
fn validate_cii_sr_454(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-454",
            "[CII-SR-454] - Only one ApplicableTradeTax should be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:Reference)
fn validate_cii_sr_442(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-442",
            "[CII-SR-442] - Reference should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:RoleCode)
fn validate_cii_sr_222(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-222",
            "[CII-SR-222] - RoleCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:SpecifiedLegalOrganization/ram:LegalClassificationCode)
fn validate_cii_sr_223(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-223",
            "[CII-SR-223] - LegalClassificationCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:SpecifiedLegalOrganization/ram:Name)
fn validate_cii_sr_224(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-224",
            "[CII-SR-224] - Name should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:SpecifiedLegalOrganization/ram:PostalTradeAddress)
fn validate_cii_sr_225(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-225",
            "[CII-SR-225] - PostalTradeAddress should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:SpecifiedLegalOrganization/ram:AuthorizedLegalRegistration)
fn validate_cii_sr_226(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-226",
            "[CII-SR-226] - RoleCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:ID)
fn validate_cii_sr_227(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-227",
            "[CII-SR-227] - ID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:TypeCode)
fn validate_cii_sr_228(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-228",
            "[CII-SR-228] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:JobTitle)
fn validate_cii_sr_229(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-229",
            "[CII-SR-229] - JobTitle should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:Responsibility)
fn validate_cii_sr_230(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-230",
            "[CII-SR-230] - Responsibility should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:PersonID)
fn validate_cii_sr_231(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-231",
            "[CII-SR-231] - PersonID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:TelephoneUniversalCommunication/ram:URIID)
fn validate_cii_sr_232(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-232",
            "[CII-SR-232] - URIID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:TelephoneUniversalCommunication/ram:ChannelCode)
fn validate_cii_sr_233(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-233",
            "[CII-SR-233] - ChannelCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:DirectTelephoneUniversalCommunication)
fn validate_cii_sr_234(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-234",
            "[CII-SR-234] - DirectTelephoneUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:MobileTelephoneUniversalCommunication)
fn validate_cii_sr_235(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-235",
            "[CII-SR-235] - MobileTelephoneUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:FaxUniversalCommunication)
fn validate_cii_sr_236(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-236",
            "[CII-SR-236] - FaxUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:EmailURIUniversalCommunication/ram:ChannelCode)
fn validate_cii_sr_237(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-237",
            "[CII-SR-237] - ChannelCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:EmailURIUniversalCommunication/ram:CompleteNumber)
fn validate_cii_sr_238(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-238",
            "[CII-SR-238] - CompleteNumber should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:TelexUniversalCommunication)
fn validate_cii_sr_239(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-239",
            "[CII-SR-239] - TelexUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:VOIPUniversalCommunication)
fn validate_cii_sr_240(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-240",
            "[CII-SR-240] - VOIPUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:InstantMessagingUniversalCommunication)
fn validate_cii_sr_241(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-241",
            "[CII-SR-241] - InstantMessagingUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:SpecifiedNote)
fn validate_cii_sr_242(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-242",
            "[CII-SR-242] - SpecifiedNote should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:DefinedTradeContact/ram:SpecifiedContactPerson)
fn validate_cii_sr_243(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-243",
            "[CII-SR-243] - SpecifiedContactPerson should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:URIUniversalCommunication/ram:ChannelCode)
fn validate_cii_sr_244(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-244",
            "[CII-SR-244] - ChannelCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:URIUniversalCommunication/ram:CompleteNumber)
fn validate_cii_sr_245(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-245",
            "[CII-SR-245] - CompleteNumber should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:SpecifiedTaxRegistration/ram:AssociatedRegisteredTax)
fn validate_cii_sr_246(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-246",
            "[CII-SR-246] - AssociatedRegisteredTax should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:EndPointURIUniversalCommunication)
fn validate_cii_sr_247(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-247",
            "[CII-SR-247] - EndPointURIUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTradeParty/ram:LogoAssociatedSpecifiedBinaryFile)
fn validate_cii_sr_248(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-248",
            "[CII-SR-248] - LogoAssociatedSpecifiedBinaryFile should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:RoleCode)
fn validate_cii_sr_249(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-249",
            "[CII-SR-249] - RoleCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:Description)
fn validate_cii_sr_250(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-250",
            "[CII-SR-250] - Description should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:SpecifiedLegalOrganization/ram:LegalClassificationCode)
fn validate_cii_sr_251(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-251",
            "[CII-SR-251] - LegalClassificationCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:SpecifiedLegalOrganization/ram:Name)
fn validate_cii_sr_252(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-252",
            "[CII-SR-252] - Name should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:SpecifiedLegalOrganization/ram:PostalTradeAddress)
fn validate_cii_sr_254(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-254",
            "[CII-SR-254] - PostalTradeAddress should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:SpecifiedLegalOrganization/ram:AuthorizedLegalRegistration)
fn validate_cii_sr_255(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-255",
            "[CII-SR-255] - AuthorizedLegalRegistration should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:ID)
fn validate_cii_sr_256(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-256",
            "[CII-SR-256] - ID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:TypeCode)
fn validate_cii_sr_257(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-257",
            "[CII-SR-257] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:JobTitle)
fn validate_cii_sr_258(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-258",
            "[CII-SR-258] - JobTitle should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:Responsibility)
fn validate_cii_sr_259(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-259",
            "[CII-SR-259] - Responsibility should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:PersonID)
fn validate_cii_sr_260(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-260",
            "[CII-SR-260] - PersonID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:TelephoneUniversalCommunication/ram:URIID)
fn validate_cii_sr_261(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-261",
            "[CII-SR-261] - URIID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:TelephoneUniversalCommunication/ram:ChannelCode)
fn validate_cii_sr_262(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-262",
            "[CII-SR-262] - ChannelCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:DirectTelephoneUniversalCommunication)
fn validate_cii_sr_263(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-263",
            "[CII-SR-263] - DirectTelephoneUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:MobileTelephoneUniversalCommunication)
fn validate_cii_sr_264(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-264",
            "[CII-SR-264] - MobileTelephoneUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:FaxUniversalCommunication)
fn validate_cii_sr_265(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-265",
            "[CII-SR-265] - FaxUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:EmailURIUniversalCommunication/ram:ChannelCode)
fn validate_cii_sr_266(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-266",
            "[CII-SR-266] - ChannelCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:EmailURIUniversalCommunication/ram:CompleteNumber)
fn validate_cii_sr_267(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-267",
            "[CII-SR-267] - CompleteNumber should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:TelexUniversalCommunication)
fn validate_cii_sr_268(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-268",
            "[CII-SR-268] - TelexUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:VOIPUniversalCommunication)
fn validate_cii_sr_269(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-269",
            "[CII-SR-269] - VOIPUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:InstantMessagingUniversalCommunication)
fn validate_cii_sr_270(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-270",
            "[CII-SR-270] - InstantMessagingUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:SpecifiedNote)
fn validate_cii_sr_271(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-271",
            "[CII-SR-271] - SpecifiedNote should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:DefinedTradeContact/ram:SpecifiedContactPerson)
fn validate_cii_sr_272(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-272",
            "[CII-SR-272] - SpecifiedContactPerson should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:URIUniversalCommunication/ram:ChannelCode)
fn validate_cii_sr_273(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-273",
            "[CII-SR-273] - ChannelCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:URIUniversalCommunication/ram:CompleteNumber)
fn validate_cii_sr_274(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-274",
            "[CII-SR-274] - CompleteNumber should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:SpecifiedTaxRegistration/ram:AssociatedRegisteredTax)
fn validate_cii_sr_275(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-275",
            "[CII-SR-275] - AssociatedRegisteredTax should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:EndPointURIUniversalCommunication)
fn validate_cii_sr_276(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-276",
            "[CII-SR-276] - EndPointURIUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerTradeParty/ram:LogoAssociatedSpecifiedBinaryFile)
fn validate_cii_sr_277(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-277",
            "[CII-SR-277] - LogoAssociatedSpecifiedBinaryFile should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SalesAgentTradeParty)
fn validate_cii_sr_278(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-278",
            "[CII-SR-278] - SalesAgentTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerRequisitionerTradeParty)
fn validate_cii_sr_279(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-279",
            "[CII-SR-279] - BuyerRequisitionerTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerAssignedAccountantTradeParty)
fn validate_cii_sr_280(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-280",
            "[CII-SR-280] - BuyerAssignedAccountantTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerAssignedAccountantTradeParty)
fn validate_cii_sr_281(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-281",
            "[CII-SR-281] - SellerAssignedAccountantTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTaxRepresentativeTradeParty/ram:ID)
fn validate_cii_sr_282(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-282",
            "[CII-SR-282] - BuyerTaxRepresentativeTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTaxRepresentativeTradeParty/ram:GlobalID)
fn validate_cii_sr_283(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-283",
            "[CII-SR-283] - GlobalID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTaxRepresentativeTradeParty/ram:RoleCode)
fn validate_cii_sr_284(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-284",
            "[CII-SR-284] - RoleCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTaxRepresentativeTradeParty/ram:Description)
fn validate_cii_sr_285(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-285",
            "[CII-SR-285] - Description should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedLegalOrganization)
fn validate_cii_sr_286(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-286",
            "[CII-SR-286] - SpecifiedLegalOrganization should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTaxRepresentativeTradeParty/ram:DefinedTradeContact)
fn validate_cii_sr_287(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-287",
            "[CII-SR-287] - DefinedTradeContact should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTaxRepresentativeTradeParty/ram:URIUniversalCommunication)
fn validate_cii_sr_288(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-288",
            "[CII-SR-288] - URIUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTaxRepresentativeTradeParty/ram:SpecifiedTaxRegistration/ram:AssociatedRegisteredTax)
fn validate_cii_sr_289(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-289",
            "[CII-SR-289] - AssociatedRegisteredTax should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTaxRepresentativeTradeParty/ram:EndPointURIUniversalCommunication)
fn validate_cii_sr_290(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-290",
            "[CII-SR-290] - EndPointURIUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerTaxRepresentativeTradeParty/ram:LogoAssociatedSpecifiedBinaryFile)
fn validate_cii_sr_291(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-291",
            "[CII-SR-291] - LogoAssociatedSpecifiedBinaryFile should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:ProductEndUserTradeParty)
fn validate_cii_sr_292(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-292",
            "[CII-SR-292] - ProductEndUserTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:ApplicableTradeDeliveryTerms)
fn validate_cii_sr_293(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-293",
            "[CII-SR-293] - ApplicableTradeDeliveryTerms should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SellerOrderReferencedDocument/ram:LineID)
fn validate_cii_sr_294(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-294",
            "[CII-SR-294] - LineID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerOrderReferencedDocument/ram:LineID)
fn validate_cii_sr_295(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-295",
            "[CII-SR-295] - LineID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:QuotationReferencedDocument)
fn validate_cii_sr_296(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-296",
            "[CII-SR-296] - QuotationReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:OrderResponseReferencedDocument)
fn validate_cii_sr_297(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-297",
            "[CII-SR-297] - OrderResponseReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:ContractReferencedDocument/ram:LineID)
fn validate_cii_sr_298(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-298",
            "[CII-SR-298] - LineID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:DemandForecastReferencedDocument)
fn validate_cii_sr_299(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-299",
            "[CII-SR-299] - DemandForecastReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SupplyInstructionReferencedDocument)
fn validate_cii_sr_300(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-300",
            "[CII-SR-300] - SupplyInstructionReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:PromotionalDealReferencedDocument)
fn validate_cii_sr_301(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-301",
            "[CII-SR-301] - PromotionalDealReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:PriceListReferencedDocument)
fn validate_cii_sr_302(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-302",
            "[CII-SR-302] - PriceListReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:AdditionalReferencedDocument/ram:LineID)
fn validate_cii_sr_303(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-303",
            "[CII-SR-303] - LineID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:RequisitionerReferencedDocument)
fn validate_cii_sr_304(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-304",
            "[CII-SR-304] - RequisitionerReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:BuyerAgentTradeParty)
fn validate_cii_sr_305(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-305",
            "[CII-SR-305] - BuyerAgentTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:PurchaseConditionsReferencedDocument)
fn validate_cii_sr_306(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-306",
            "[CII-SR-306] - PurchaseConditionsReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:SpecifiedProcuringProject/ram:Description)
fn validate_cii_sr_307(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-307",
            "[CII-SR-307] - Description should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: not(ram:UltimateCustomerOrderReferencedDocument)
fn validate_cii_sr_448(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-448",
            "[CII-SR-448] - UltimateCustomerOrderReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: (not(ram:BuyerTradeParty/ram:ID) and ram:BuyerTradeParty/ram:GlobalID) or (ram:BuyerTradeParty/ram:ID and not(ram:BuyerTradeParty/ram:GlobalID)) or (not(ram:BuyerTradeParty/ram:ID) and not(ram:BuyerTradeParty/ram:GlobalID))
fn validate_cii_sr_450(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-450",
            "[CII-SR-450] - Only one  buyer identifier should be present (either the ID or the Global ID)",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: ( count(ram:SellerTradeParty/ram:DefinedTradeContact)  <= 1)
fn validate_cii_sr_455(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-455",
            "[CII-SR-455] - DefinedTradeContact of SellerTradeParty shall exist maximum once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: ( count(ram:BuyerTradeParty/ram:DefinedTradeContact)  <= 1)
fn validate_cii_sr_456(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-456",
            "[CII-SR-456] - DefinedTradeContact of BuyerTradeParty shall exist maximum once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: ( count(ram:AdditionalReferencedDocument[ram:TypeCode='50'])  <= 1)
fn validate_cii_sr_457(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-457",
            "[CII-SR-457] - IssuerAssignedID with TypeCode 50 should exist maximum once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: ( count(ram:AdditionalReferencedDocument[ram:TypeCode='130'])  <= 1)
fn validate_cii_sr_458(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-458",
            "[CII-SR-458] - IssuerAssignedID with TypeCode 130 should exist maximum once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: count(ram:SellerTradeParty/ram:URIUniversalCommunication) <= 1
fn validate_cii_sr_459(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-459",
            "[CII-SR-459] - SellerTradeParty URIUniversalCommunication should exist maximum once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement
// Test: count(ram:BuyerTradeParty/ram:URIUniversalCommunication) <= 1
fn validate_cii_sr_460(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-460",
            "[CII-SR-460] - BuyerTradeParty URIUniversalCommunication should exist maximum once",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:RelatedSupplyChainConsignment)
fn validate_cii_sr_308(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-308",
            "[CII-SR-308] - RelatedSupplyChainConsignment should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ShipToTradeParty/ram:RoleCode)
fn validate_cii_sr_309(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-309",
            "[CII-SR-309] - RoleCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ShipToTradeParty/ram:Description)
fn validate_cii_sr_310(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-310",
            "[CII-SR-310] - Description should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ShipToTradeParty/ram:SpecifiedLegalOrganization)
fn validate_cii_sr_311(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-311",
            "[CII-SR-311] - SpecifiedLegalOrganization should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ShipToTradeParty/ram:DefinedTradeContact)
fn validate_cii_sr_312(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-312",
            "[CII-SR-312] - DefinedTradeContact should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ShipToTradeParty/ram:URIUniversalCommunication)
fn validate_cii_sr_313(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-313",
            "[CII-SR-313] - URIUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ShipToTradeParty/ram:SpecifiedTaxRegistration)
fn validate_cii_sr_314(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-314",
            "[CII-SR-314] - SpecifiedTaxRegistration should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ShipToTradeParty/ram:EndPointURIUniversalCommunication)
fn validate_cii_sr_315(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-315",
            "[CII-SR-315] - EndPointURIUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ShipToTradeParty/ram:LogoAssociatedSpecifiedBinaryFile)
fn validate_cii_sr_316(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-316",
            "[CII-SR-316] - LogoAssociatedSpecifiedBinaryFile should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:UltimateShipToTradeParty)
fn validate_cii_sr_317(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-317",
            "[CII-SR-317] - UltimateShipToTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ShipFromTradeParty)
fn validate_cii_sr_318(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-318",
            "[CII-SR-318] - ShipFromTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ActualDespatchSupplyChainEvent)
fn validate_cii_sr_319(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-319",
            "[CII-SR-319] - ActualDespatchSupplyChainEvent should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ActualPickUpSupplyChainEvent)
fn validate_cii_sr_320(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-320",
            "[CII-SR-320] - ActualPickUpSupplyChainEvent should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ActualDeliverySupplyChainEvent/ram:ID)
fn validate_cii_sr_321(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-321",
            "[CII-SR-321] - ID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ActualDeliverySupplyChainEvent/ram:OccurrenceDateTime/udt:DateTime)
fn validate_cii_sr_322(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-322",
            "[CII-SR-322] - DateTime should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ActualDeliverySupplyChainEvent/ram:TypeCode)
fn validate_cii_sr_323(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-323",
            "[CII-SR-323] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ActualDeliverySupplyChainEvent/ram:Description)
fn validate_cii_sr_324(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-324",
            "[CII-SR-324] - Description should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ActualDeliverySupplyChainEvent/ram:DescriptionBinaryObject)
fn validate_cii_sr_325(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-325",
            "[CII-SR-325] - DescriptionBinaryObject should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ActualDeliverySupplyChainEvent/ram:UnitQuantity)
fn validate_cii_sr_326(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-326",
            "[CII-SR-326] - UnitQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ActualDeliverySupplyChainEvent/ram:LatestOccurrenceDateTime)
fn validate_cii_sr_327(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-327",
            "[CII-SR-327] - LatestOccurrenceDateTime should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ActualDeliverySupplyChainEvent/ram:EarliestOccurrenceDateTime)
fn validate_cii_sr_328(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-328",
            "[CII-SR-328] - EarliestOccurrenceDateTime should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ActualDeliverySupplyChainEvent/ram:OccurrenceSpecifiedPeriod)
fn validate_cii_sr_329(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-329",
            "[CII-SR-329] - OccurrenceSpecifiedPeriod should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ActualDeliverySupplyChainEvent/ram:OccurrenceLogisticsLocation)
fn validate_cii_sr_330(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-330",
            "[CII-SR-330] - OccurrenceLogisticsLocation should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ActualReceiptSupplyChainEvent)
fn validate_cii_sr_331(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-331",
            "[CII-SR-331] - ActualReceiptSupplyChainEvent should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:AdditionalReferencedDocument)
fn validate_cii_sr_332(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-332",
            "[CII-SR-332] - AdditionalReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:DespatchAdviceReferencedDocument/ram:LineID)
fn validate_cii_sr_333(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-333",
            "[CII-SR-333] - LineID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:DespatchAdviceReferencedDocument/ram:LineID)
fn validate_cii_sr_334(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-334",
            "[CII-SR-334] - LineID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:DeliveryNoteReferencedDocument)
fn validate_cii_sr_335(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-335",
            "[CII-SR-335] - DeliveryNoteReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:ConsumptionReportReferencedDocument)
fn validate_cii_sr_336(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-336",
            "[CII-SR-336] - ConsumptionReportReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:PreviousDeliverySupplyChainEvent)
fn validate_cii_sr_337(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-337",
            "[CII-SR-337] - PreviousDeliverySupplyChainEvent should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: not(ram:PackingListReferencedDocument)
fn validate_cii_sr_338(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-338",
            "[CII-SR-338] - PackingListReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeDelivery
// Test: (not(ram:ShipToTradeParty/ram:ID) and ram:ShipToTradeParty/ram:GlobalID) or (ram:ShipToTradeParty/ram:ID and not(ram:ShipToTradeParty/ram:GlobalID)) or (not(ram:ShipToTradeParty/ram:ID) and not(ram:ShipToTradeParty/ram:GlobalID))
fn validate_cii_sr_449(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-449",
            "[CII-SR-449] - Only one delivery to location identifier should be present (either the ID or the Global ID)",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:DuePayableAmount)
fn validate_cii_sr_339(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-339",
            "[CII-SR-339] - DuePayableAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:CreditorReferenceTypeCode)
fn validate_cii_sr_340(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-340",
            "[CII-SR-340] - CreditorReferenceTypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:CreditorReferenceType)
fn validate_cii_sr_341(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-341",
            "[CII-SR-341] - CreditorReferenceType should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:CreditorReferenceIssuerID)
fn validate_cii_sr_342(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-342",
            "[CII-SR-342] - CreditorReferenceIssuerID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PaymentCurrencyCode)
fn validate_cii_sr_344(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-344",
            "[CII-SR-344] - PaymentCurrencyCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:InvoiceIssuerReference)
fn validate_cii_sr_345(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-345",
            "[CII-SR-345] - InvoiceIssuerReference should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:InvoiceDateTime)
fn validate_cii_sr_346(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-346",
            "[CII-SR-346] - InvoiceDateTime should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:NextInvoiceDateTime)
fn validate_cii_sr_347(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-347",
            "[CII-SR-347] - NextInvoiceDateTime should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:CreditReasonCode)
fn validate_cii_sr_348(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-348",
            "[CII-SR-348] - CreditReasonCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:CreditReason)
fn validate_cii_sr_349(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-349",
            "[CII-SR-349] - CreditReason should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:InvoicerTradeParty)
fn validate_cii_sr_350(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-350",
            "[CII-SR-350] - InvoicerTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:InvoiceeTradeParty)
fn validate_cii_sr_351(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-351",
            "[CII-SR-351] - InvoiceeTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayeeTradeParty/ram:RoleCode)
fn validate_cii_sr_352(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-352",
            "[CII-SR-352] - RoleCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayeeTradeParty/ram:Description)
fn validate_cii_sr_353(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-353",
            "[CII-SR-353] - Description should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayeeTradeParty/ram:SpecifiedLegalOrganization/ram:LegalClassificationCode)
fn validate_cii_sr_354(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-354",
            "[CII-SR-354] - LegalClassificationCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayeeTradeParty/ram:SpecifiedLegalOrganization/ram:Name)
fn validate_cii_sr_355(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-355",
            "[CII-SR-355] - Name should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayeeTradeParty/ram:SpecifiedLegalOrganization/ram:TradingBusinessName)
fn validate_cii_sr_356(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-356",
            "[CII-SR-356] - TradingBusinessName should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayeeTradeParty/ram:SpecifiedLegalOrganization/ram:PostalTradeAddress)
fn validate_cii_sr_357(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-357",
            "[CII-SR-357] - PostalTradeAddress should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayeeTradeParty/ram:SpecifiedLegalOrganization/ram:AuthorizedLegalRegistration)
fn validate_cii_sr_358(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-358",
            "[CII-SR-358] - AuthorizedLegalRegistration should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayeeTradeParty/ram:DefinedTradeContact)
fn validate_cii_sr_359(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-359",
            "[CII-SR-359] - DefinedTradeContact should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayeeTradeParty/ram:PostalTradeAddress)
fn validate_cii_sr_360(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-360",
            "[CII-SR-360] - PostalTradeAddress should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayeeTradeParty/ram:URIUniversalCommunication)
fn validate_cii_sr_361(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-361",
            "[CII-SR-361] - URIUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayeeTradeParty/ram:SpecifiedTaxRegistration)
fn validate_cii_sr_362(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-362",
            "[CII-SR-362] - SpecifiedTaxRegistration should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayeeTradeParty/ram:EndPointURIUniversalCommunication)
fn validate_cii_sr_363(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-363",
            "[CII-SR-363] - EndPointURIUniversalCommunication should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayeeTradeParty/ram:LogoAssociatedSpecifiedBinaryFile)
fn validate_cii_sr_364(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-364",
            "[CII-SR-364] - LogoAssociatedSpecifiedBinaryFile should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: (not(ram:PayeeTradeParty/ram:ID) and ram:PayeeTradeParty/ram:GlobalID) or (ram:PayeeTradeParty/ram:ID and not(ram:PayeeTradeParty/ram:GlobalID)) or (not(ram:PayeeTradeParty/ram:ID) and not(ram:PayeeTradeParty/ram:GlobalID))
fn validate_cii_sr_451(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-451",
            "[CII-SR-451] - Only one payee identifier should be present (either the ID or the Global ID)",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayerTradeParty)
fn validate_cii_sr_365(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-365",
            "[CII-SR-365] - PayerTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:TaxApplicableTradeCurrencyExchange)
fn validate_cii_sr_366(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-366",
            "[CII-SR-366] - TaxApplicableTradeCurrencyExchange should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:InvoiceApplicableTradeCurrencyExchange)
fn validate_cii_sr_367(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-367",
            "[CII-SR-367] - InvoiceApplicableTradeCurrencyExchange should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PaymentApplicableTradeCurrencyExchange)
fn validate_cii_sr_368(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-368",
            "[CII-SR-368] - PaymentApplicableTradeCurrencyExchange should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:PaymentChannelCode)
fn validate_cii_sr_369(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-369",
            "[CII-SR-369] - PaymentChannelCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:GuaranteeMethodCode)
fn validate_cii_sr_370(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-370",
            "[CII-SR-370] - GuaranteeMethodCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:PaymentMethodCode)
fn validate_cii_sr_371(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-371",
            "[CII-SR-371] - PaymentMethodCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:ID)
fn validate_cii_sr_443(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-443",
            "[CII-SR-443] - ID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:ApplicableTradeSettlementFinancialCard/ram:MicrochipIndicator)
fn validate_cii_sr_372(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-372",
            "[CII-SR-372] - MicrochipIndicator should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:ApplicableTradeSettlementFinancialCard/ram:TypeCode)
fn validate_cii_sr_373(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-373",
            "[CII-SR-373] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:ApplicableTradeSettlementFinancialCard/ram:ExpiryDate)
fn validate_cii_sr_375(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-375",
            "[CII-SR-375] - ExpiryDate should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:ApplicableTradeSettlementFinancialCard/ram:VerificationNumeric)
fn validate_cii_sr_376(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-376",
            "[CII-SR-376] - VerificationNumeric should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:ApplicableTradeSettlementFinancialCard/ram:ValidFromDateTime)
fn validate_cii_sr_377(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-377",
            "[CII-SR-377] - ValidFromDateTime should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:ApplicableTradeSettlementFinancialCard/ram:CreditLimitAmount)
fn validate_cii_sr_378(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-378",
            "[CII-SR-378] - CreditLimitAmountshould not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:ApplicableTradeSettlementFinancialCard/ram:CreditAvailableAmount)
fn validate_cii_sr_379(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-379",
            "[CII-SR-379] - CreditAvailableAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:ApplicableTradeSettlementFinancialCard/ram:InterestRatePercent)
fn validate_cii_sr_380(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-380",
            "[CII-SR-380] - InterestRatePercent should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:ApplicableTradeSettlementFinancialCard/ram:Description)
fn validate_cii_sr_381(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-381",
            "[CII-SR-381] - Description should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:PayerPartyDebtorFinancialAccount/ram:AccountName)
fn validate_cii_sr_382(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-382",
            "[CII-SR-382] - AccountName should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:PayerPartyDebtorFinancialAccount/ram:ProprietaryID)
fn validate_cii_sr_444(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-444",
            "[CII-SR-444] - ProprietaryID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:PayerSpecifiedDebtorFinancialInstitution/ram:ClearingSystemName)
fn validate_cii_sr_384(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-384",
            "[CII-SR-384] - ClearingSystemName should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:PayerSpecifiedDebtorFinancialInstitution/ram:Name)
fn validate_cii_sr_385(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-385",
            "[CII-SR-385] - Name should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementPaymentMeans/ram:PayerSpecifiedDebtorFinancialInstitution/ram:LocationFinancialInstitutionAddress)
fn validate_cii_sr_386(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-386",
            "[CII-SR-386] - LocationFinancialInstitutionAddress should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:ID)
fn validate_cii_sr_388(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-388",
            "[CII-SR-388] - ID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:SequenceNumeric)
fn validate_cii_sr_389(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-389",
            "[CII-SR-389] - SequenceNumeric should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:BasisQuantity)
fn validate_cii_sr_390(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-390",
            "[CII-SR-390] - BasisQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:PrepaidIndicator)
fn validate_cii_sr_391(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-391",
            "[CII-SR-391] - PrepaidIndicator should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:UnitBasisAmount)
fn validate_cii_sr_392(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-392",
            "[CII-SR-392] - UnitBasisAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:TypeCode)
fn validate_cii_sr_393(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-393",
            "[CII-SR-393] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeAllowanceCharge/ram:ActualTradeCurrencyExchange)
fn validate_cii_sr_394(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-394",
            "[CII-SR-394] - ActualTradeCurrencyExchange should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SubtotalCalculatedTradeTax)
fn validate_cii_sr_395(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-395",
            "[CII-SR-395] - SubtotalCalculatedTradeTax should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedLogisticsServiceCharge)
fn validate_cii_sr_396(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-396",
            "[CII-SR-396] - SpecifiedLogisticsServiceCharge should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradePaymentTerms/ram:ID)
fn validate_cii_sr_397(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-397",
            "[CII-SR-397] - ID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradePaymentTerms/ram:FromEventCode)
fn validate_cii_sr_398(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-398",
            "[CII-SR-398] - FromEventCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradePaymentTerms/ram:SettlementPeriodMeasure)
fn validate_cii_sr_399(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-399",
            "[CII-SR-399] - SettlementPeriodMeasure should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradePaymentTerms/ram:DueDateDateTime/udt:DateTime)
fn validate_cii_sr_400(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-400",
            "[CII-SR-400] - DateTime should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradePaymentTerms/ram:TypeCode)
fn validate_cii_sr_401(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-401",
            "[CII-SR-401] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradePaymentTerms/ram:InstructionTypeCode)
fn validate_cii_sr_402(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-402",
            "[CII-SR-402] - InstructionTypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradePaymentTerms/ram:PartialPaymentPercent)
fn validate_cii_sr_404(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-404",
            "[CII-SR-404] - PartialPaymentPercent should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradePaymentTerms/ram:PaymentMeansID)
fn validate_cii_sr_405(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-405",
            "[CII-SR-405] - PaymentMeansID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradePaymentTerms/ram:PartialPaymentAmount)
fn validate_cii_sr_406(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-406",
            "[CII-SR-406] - PartialPaymentAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradePaymentTerms/ram:ApplicableTradePaymentPenaltyTerms)
fn validate_cii_sr_407(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-407",
            "[CII-SR-407] - ApplicableTradePaymentPenaltyTerms should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradePaymentTerms/ram:ApplicableTradePaymentDiscountTerms)
fn validate_cii_sr_408(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-408",
            "[CII-SR-408] - ApplicableTradePaymentDiscountTerms should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradePaymentTerms/ram:PayeeTradeParty)
fn validate_cii_sr_409(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-409",
            "[CII-SR-409] - PayeeTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedFinancialAdjustment)
fn validate_cii_sr_421(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-421",
            "[CII-SR-421] - SpecifiedFinancialAdjustment should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:InvoiceReferencedDocument/ram:LineID)
fn validate_cii_sr_422(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-422",
            "[CII-SR-422] - LineID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:ProFormaInvoiceReferencedDocument)
fn validate_cii_sr_423(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-423",
            "[CII-SR-423] - ProFormaInvoiceReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:LetterOfCreditReferencedDocument)
fn validate_cii_sr_424(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-424",
            "[CII-SR-424] - LetterOfCreditReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:FactoringAgreementReferencedDocument)
fn validate_cii_sr_425(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-425",
            "[CII-SR-425] - FactoringAgreementReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:FactoringListReferencedDocument)
fn validate_cii_sr_426(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-426",
            "[CII-SR-426] - FactoringListReferencedDocument should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PayableSpecifiedTradeAccountingAccount)
fn validate_cii_sr_427(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-427",
            "[CII-SR-427] - PayableSpecifiedTradeAccountingAccount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:ReceivableSpecifiedTradeAccountingAccount/ram:SetTriggerCode)
fn validate_cii_sr_428(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-428",
            "[CII-SR-428] - SetTriggerCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:ReceivableSpecifiedTradeAccountingAccount/ram:TypeCode)
fn validate_cii_sr_429(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-429",
            "[CII-SR-429] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:ReceivableSpecifiedTradeAccountingAccount/ram:AmountTypeCode)
fn validate_cii_sr_430(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-430",
            "[CII-SR-430] - AmountTypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:ReceivableSpecifiedTradeAccountingAccount/ram:Name)
fn validate_cii_sr_431(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-431",
            "[CII-SR-431] - Name should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:ReceivableSpecifiedTradeAccountingAccount/ram:CostReferenceDimensionPattern)
fn validate_cii_sr_432(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-432",
            "[CII-SR-432] - CostReferenceDimensionPattern should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:PurchaseSpecifiedTradeAccountingAccount)
fn validate_cii_sr_433(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-433",
            "[CII-SR-433] - PurchaseSpecifiedTradeAccountingAccount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SalesSpecifiedTradeAccountingAccount)
fn validate_cii_sr_434(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-434",
            "[CII-SR-434] - SalesSpecifiedTradeAccountingAccount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedTradeSettlementFinancialCard)
fn validate_cii_sr_435(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-435",
            "[CII-SR-435] - SpecifiedTradeSettlementFinancialCard should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:SpecifiedAdvancePayment)
fn validate_cii_sr_436(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-436",
            "[CII-SR-436] - SpecifiedAdvancePayment should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: not(ram:UltimatePayeeTradeParty)
fn validate_cii_sr_437(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-437",
            "[CII-SR-437] - UltimatePayeeTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: count(ram:SpecifiedTradePaymentTerms) <= 1
fn validate_cii_sr_452(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-452",
            "[CII-SR-452] - Only one SpecifiedTradePaymentTerms should be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: count(ram:SpecifiedTradePaymentTerms/ram:Description) <= 1
fn validate_cii_sr_453(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-453",
            "[CII-SR-453] - Only one SpecifiedTradePaymentTerms Description should be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: count(ram:ApplicableTradeTax/ram:TaxPointDate) <= 1
fn validate_cii_sr_461(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-461",
            "[CII-SR-461] - Only one TaxPointDate shall be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement
// Test: count(//ram:ApplicableTradeTax/ram:DueDateTypeCode) = 0 or count(distinct-values(//ram:ApplicableTradeTax/ram:DueDateTypeCode)) = 1
fn validate_cii_sr_462(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-SR-462",
            "[CII-SR-462] - Only one DueDateTypeCode shall be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: not(ram:InformationAmount)
fn validate_cii_sr_411(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-411",
            "[CII-SR-411] - InformationAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: not(ram:TotalDiscountAmount)
fn validate_cii_sr_412(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-412",
            "[CII-SR-412] - TotalDiscountAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: not(ram:TotalAllowanceChargeAmount)
fn validate_cii_sr_413(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-413",
            "[CII-SR-413] - TotalAllowanceChargeAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: not(ram:RetailValueExcludingTaxInformationAmount)
fn validate_cii_sr_414(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-414",
            "[CII-SR-414] - RetailValueExcludingTaxInformationAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: not(ram:TotalDepositFeeInformationAmount)
fn validate_cii_sr_415(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-415",
            "[CII-SR-415] - TotalDepositFeeInformationAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: not(ram:ProductValueExcludingTobaccoTaxInformationAmount)
fn validate_cii_sr_416(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-416",
            "[CII-SR-416] - ProductValueExcludingTobaccoTaxInformationAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: not(ram:TotalRetailValueInformationAmount)
fn validate_cii_sr_417(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-417",
            "[CII-SR-417] - TotalRetailValueInformationAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: not(ram:GrossLineTotalAmount)
fn validate_cii_sr_418(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-418",
            "[CII-SR-418] - GrossLineTotalAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: not(ram:NetLineTotalAmount)
fn validate_cii_sr_419(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-419",
            "[CII-SR-419] - NetLineTotalAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement/ram:SpecifiedTradeSettlementHeaderMonetarySummation
// Test: not(ram:NetIncludingTaxesLineTotalAmount)
fn validate_cii_sr_420(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-420",
            "[CII-SR-420] - NetIncludingTaxesLineTotalAmount should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: not(@languageID)
fn validate_cii_dt_013(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-013",
            "[CII-DT-013] - languageID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: not(@languageLocaleID)
fn validate_cii_dt_014(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-014",
            "[CII-DT-014] - languageLocaleID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice
// Test: not(ram:ValuationBreakdownStatement)
fn validate_cii_sr_438(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-438",
            "[CII-SR-438] - ValuationBreakdownStatement should not be present",
        )));
    }
    Ok(())
}

// Context: //*[ends-with(name(), 'DocumentContextParameter')]
// Test: not(ram:Value)
fn validate_cii_sr_04(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-04",
            "[CII-SR-04] - Value should not be present",
        )));
    }
    Ok(())
}

// Context: //*[ends-with(name(), 'DocumentContextParameter')]
// Test: not(ram:SpecifiedDocumentVersion)
fn validate_cii_sr_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-SR-05",
            "[CII-SR-05] - SpecifiedDocumentVersion should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext/ram:GuidelineSpecifiedDocumentContextParameter/ram:ID |            /rsm:CrossIndustryInvoice/rsm:ExchangedDocument/ram:ID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:AssociatedDocumentLineDocument/ram:LineID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct/ram:SellerAssignedID
// Test: not(@schemeName)
fn validate_cii_dt_001(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-001",
            "[CII-DT-001] - schemeName should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext/ram:GuidelineSpecifiedDocumentContextParameter/ram:ID |            /rsm:CrossIndustryInvoice/rsm:ExchangedDocument/ram:ID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:AssociatedDocumentLineDocument/ram:LineID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct/ram:SellerAssignedID
// Test: not(@schemeAgencyName)
fn validate_cii_dt_002(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-002",
            "[CII-DT-002] - schemeAgencyName should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext/ram:GuidelineSpecifiedDocumentContextParameter/ram:ID |            /rsm:CrossIndustryInvoice/rsm:ExchangedDocument/ram:ID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:AssociatedDocumentLineDocument/ram:LineID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct/ram:SellerAssignedID
// Test: not(@schemeDataURI)
fn validate_cii_dt_003(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-003",
            "[CII-DT-003] - schemeDataURI should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext/ram:GuidelineSpecifiedDocumentContextParameter/ram:ID |            /rsm:CrossIndustryInvoice/rsm:ExchangedDocument/ram:ID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:AssociatedDocumentLineDocument/ram:LineID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct/ram:SellerAssignedID
// Test: not(@schemeURI)
fn validate_cii_dt_004(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-004",
            "[CII-DT-004] - schemeURI should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext/ram:GuidelineSpecifiedDocumentContextParameter/ram:ID |            /rsm:CrossIndustryInvoice/rsm:ExchangedDocument/ram:ID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:AssociatedDocumentLineDocument/ram:LineID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct/ram:SellerAssignedID
// Test: not(@schemeID)
fn validate_cii_dt_005(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-005",
            "[CII-DT-005] - schemeID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext/ram:GuidelineSpecifiedDocumentContextParameter/ram:ID |            /rsm:CrossIndustryInvoice/rsm:ExchangedDocument/ram:ID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:AssociatedDocumentLineDocument/ram:LineID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct/ram:SellerAssignedID
// Test: not(@schemeAgencyID)
fn validate_cii_dt_006(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-006",
            "[CII-DT-006] - schemeAgencyID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocumentContext/ram:GuidelineSpecifiedDocumentContextParameter/ram:ID |            /rsm:CrossIndustryInvoice/rsm:ExchangedDocument/ram:ID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:AssociatedDocumentLineDocument/ram:LineID |            /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedTradeProduct/ram:SellerAssignedID
// Test: not(@schemeVersionID)
fn validate_cii_dt_007(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-007",
            "[CII-DT-007] - schemeVersionID should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ID')]
// Test: not(@schemeName)
fn validate_cii_dt_0010(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-0010",
            "[CII-DT-0010] - schemeName should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ID')]
// Test: not(@schemeAgencyName)
fn validate_cii_dt_0020(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-0020",
            "[CII-DT-0020] - schemeAgencyName should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ID')]
// Test: not(@schemeDataURI)
fn validate_cii_dt_0030(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-0030",
            "[CII-DT-0030] - schemeDataURI should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ID')]
// Test: not(@schemeURI)
fn validate_cii_dt_0040(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-0040",
            "[CII-DT-0040] - schemeURI should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:TypeCode
// Test: not(@name)
fn validate_cii_dt_008(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-008",
            "[CII-DT-008] - name should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:TypeCode
// Test: not(@listURI)
fn validate_cii_dt_009(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-009",
            "[CII-DT-009] - listURI should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument/ram:TypeCode
// Test: not(@listID)
fn validate_cii_dt_010(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-010",
            "[CII-DT-010] - listID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument/ram:TypeCode
// Test: not(@listAgencyID)
fn validate_cii_dt_011(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-011",
            "[CII-DT-011] - listAgencyID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:ExchangedDocument/ram:TypeCode
// Test: not(@listVersionID)
fn validate_cii_dt_012(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-012",
            "[CII-DT-012] - listVersionID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode
// Test: not(@listID)
fn validate_cii_dt_045(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-045",
            "[CII-DT-045] - @listID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode
// Test: not(@listAgencyID)
fn validate_cii_dt_046(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-046",
            "[CII-DT-046] - @listAgencyID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode
// Test: not(@listVersionID)
fn validate_cii_dt_047(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-047",
            "[CII-DT-047] - @listVersionID should not be present",
        )));
    }
    Ok(())
}

// Context: /rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeSettlement/ram:ApplicableTradeTax/ram:CategoryCode
// Test: not(@listURI)
fn validate_cii_dt_048(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-048",
            "[CII-DT-048] - @listURI should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:URIID) or (self::ram:AdditionalReferencedDocument and ram:TypeCode='916')
fn validate_cii_dt_015(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-015",
            "[CII-DT-015] - URIID should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:StatusCode)
fn validate_cii_dt_016(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-016",
            "[CII-DT-016] - StatusCode should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:CopyIndicator)
fn validate_cii_dt_017(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-017",
            "[CII-DT-017] - CopyIndicator should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:TypeCode) or (self::ram:AdditionalReferencedDocument) and (ram:TypeCode='50' or ram:TypeCode='130' or ram:TypeCode='916')
fn validate_cii_dt_018(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-018",
            "[CII-DT-018] - TypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:GlobalID)
fn validate_cii_dt_019(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-019",
            "[CII-DT-019] - GlobalID should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:RevisionID)
fn validate_cii_dt_020(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-020",
            "[CII-DT-020] - RevisionID should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:Name) or (self::ram:AdditionalReferencedDocument and ram:TypeCode='916')
fn validate_cii_dt_021(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-021",
            "[CII-DT-021] - Name should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:AttachmentBinaryObject) or (self::ram:AdditionalReferencedDocument and ram:TypeCode='916')
fn validate_cii_dt_022(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-022",
            "[CII-DT-022] - AttachmentBinaryObject should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:Information)
fn validate_cii_dt_023(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-023",
            "[CII-DT-023] - Information should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:ReferenceTypeCode) or (self::ram:AdditionalReferencedDocument and ram:TypeCode='130')
fn validate_cii_dt_024(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-024",
            "[CII-DT-024] - ReferenceTypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:SectionName)
fn validate_cii_dt_025(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-025",
            "[CII-DT-025] - SectionName should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:PreviousRevisionID)
fn validate_cii_dt_026(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-026",
            "[CII-DT-026] - PreviousRevisionID should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:FormattedIssueDateTime) or self::ram:InvoiceReferencedDocument
fn validate_cii_dt_027(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-027",
            "[CII-DT-027] - FormattedIssueDateTime should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:EffectiveSpecifiedPeriod)
fn validate_cii_dt_028(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-028",
            "[CII-DT-028] - EffectiveSpecifiedPeriod should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:IssuerTradeParty)
fn validate_cii_dt_029(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-029",
            "[CII-DT-029] - IssuerTradeParty should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'ReferencedDocument')]
// Test: not(ram:AttachedSpecifiedBinaryFile)
fn validate_cii_dt_030(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-030",
            "[CII-DT-030] - AttachedSpecifiedBinaryFile should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'Amount') and not (self::ram:TaxTotalAmount)]
// Test: not(@currencyID)
fn validate_cii_dt_031(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-031",
            "[CII-DT-031] - currencyID should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'Amount') and not (self::ram:TaxTotalAmount)]
// Test: not(@currencyCodeListVersionID)
fn validate_cii_dt_032(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-032",
            "[CII-DT-032] - currencyCodeListVersionID should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'Quantity')]
// Test: not(@unitCode) or (/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem/ram:SpecifiedLineTradeDelivery/ram:BilledQuantity/@unitCode)
fn validate_cii_dt_033(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-033",
            "[CII-DT-033] - unitCode should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'Quantity')]
// Test: not(@unitCodeListID)
fn validate_cii_dt_034(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-034",
            "[CII-DT-034] - unitCodeListID should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'Quantity')]
// Test: not(@unitCodeListAgencyID)
fn validate_cii_dt_035(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-035",
            "[CII-DT-035] - unitCodeListAgencyID should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'Quantity')]
// Test: not(@unitCodeListAgencyName)
fn validate_cii_dt_036(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-036",
            "[CII-DT-036] - unitCodeListAgencyName should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:TypeCode) or (ram:TypeCode = 'VAT')
fn validate_cii_dt_037(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-037",
            "[CII-DT-037] - TypeCode shall be 'VAT'",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:CalculatedRate)
fn validate_cii_dt_038(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-038",
            "[CII-DT-038] - CalculatedRate should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:CalculationSequenceNumeric)
fn validate_cii_dt_039(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-039",
            "[CII-DT-039] - CalculationSequenceNumeric should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:BasisQuantity)
fn validate_cii_dt_040(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-040",
            "[CII-DT-040] - BasisQuantity should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:BasisAmount) or (ancestor::ram:ApplicableHeaderTradeSettlement)
fn validate_cii_dt_041(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-041",
            "[CII-DT-041] - BasisAmount should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:UnitBasisAmount)
fn validate_cii_dt_042(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-042",
            "[CII-DT-042] - UnitBasisAmount should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:LineTotalBasisAmount)
fn validate_cii_dt_043(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-043",
            "[CII-DT-043] - LineTotalBasisAmount should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:AllowanceChargeBasisAmount)
fn validate_cii_dt_044(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-044",
            "[CII-DT-044] - AllowanceChargeBasisAmount should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:CurrencyCode)
fn validate_cii_dt_049(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-049",
            "[CII-DT-049] - CurrencyCode should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:Jurisdiction)
fn validate_cii_dt_050(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-050",
            "[CII-DT-050] - Jurisdiction should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:CustomsDutyIndicator)
fn validate_cii_dt_051(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-051",
            "[CII-DT-051] - CustomsDutyIndicator should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:ExemptionReasonCode) or self::ram:ApplicableTradeTax
fn validate_cii_dt_052(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-052",
            "[CII-DT-052] - ExemptionReasonCode should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:ExemptionReason) or self::ram:ApplicableTradeTax
fn validate_cii_dt_098(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-098",
            "[CII-DT-098] - ExemptionReason should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:TaxBasisAllowanceRate)
fn validate_cii_dt_053(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-053",
            "[CII-DT-053] - TaxBasisAllowanceRate should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:TaxPointDate)  or (ancestor::ram:ApplicableHeaderTradeSettlement)
fn validate_cii_dt_054(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-054",
            "[CII-DT-054] - TaxPointDate should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:Type)
fn validate_cii_dt_055(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-055",
            "[CII-DT-055] - Type should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:InformationAmount)
fn validate_cii_dt_056(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-056",
            "[CII-DT-056] - InformationAmount should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:CategoryName)
fn validate_cii_dt_057(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-057",
            "[CII-DT-057] - CategoryName should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:DueDateTypeCode) or (ancestor::ram:ApplicableHeaderTradeSettlement)
fn validate_cii_dt_058(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-058",
            "[CII-DT-058] - DueDateTypeCode should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:RateApplicablePercent/@format)
fn validate_cii_dt_059(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-059",
            "[CII-DT-059] - @format should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:SpecifiedTradeAccountingAccount)
fn validate_cii_dt_060(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-060",
            "[CII-DT-060] - SpecifiedTradeAccountingAccount should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:ServiceSupplyTradeCountry)
fn validate_cii_dt_061(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-061",
            "[CII-DT-061] - ServiceSupplyTradeCountry should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:BuyerRepayableTaxSpecifiedTradeAccountingAccount)
fn validate_cii_dt_062(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-062",
            "[CII-DT-062] - BuyerRepayableTaxSpecifiedTradeAccountingAccount should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:SellerPayableTaxSpecifiedTradeAccountingAccount)
fn validate_cii_dt_063(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-063",
            "[CII-DT-063] - SellerPayableTaxSpecifiedTradeAccountingAccount should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:SellerRefundableTaxSpecifiedTradeAccountingAccount)
fn validate_cii_dt_064(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-064",
            "[CII-DT-064] - SellerRefundableTaxSpecifiedTradeAccountingAccount should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:BuyerDeductibleTaxSpecifiedTradeAccountingAccount)
fn validate_cii_dt_065(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-065",
            "[CII-DT-065] - BuyerDeductibleTaxSpecifiedTradeAccountingAccount should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:BuyerNonDeductibleTaxSpecifiedTradeAccountingAccount)
fn validate_cii_dt_066(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-066",
            "[CII-DT-066] - BuyerNonDeductibleTaxSpecifiedTradeAccountingAccount should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:*[ends-with(name(), 'TradeTax')]
// Test: not(ram:PlaceApplicableTradeLocation)
fn validate_cii_dt_067(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "CII-DT-067",
            "[CII-DT-067] - PlaceApplicableTradeLocation should not be present",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:StartDateTime/udt:DateTime)
fn validate_cii_dt_068(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-068",
            "[CII-DT-068] - DateTime shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:DurationMeasure)
fn validate_cii_dt_069(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-069",
            "[CII-DT-069] - DurationMeasure shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:InclusiveIndicator)
fn validate_cii_dt_070(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-070",
            "[CII-DT-070] - InclusiveIndicator shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:Description)
fn validate_cii_dt_071(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-071",
            "[CII-DT-071] - Description shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:EndDateTime/udt:DateTime)
fn validate_cii_dt_072(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-072",
            "[CII-DT-072] - DateTime shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:CompleteDateTime)
fn validate_cii_dt_073(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-073",
            "[CII-DT-073] - CompleteDateTime shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:OpenIndicator)
fn validate_cii_dt_074(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-074",
            "[CII-DT-074] - OpenIndicator shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:SeasonCode)
fn validate_cii_dt_075(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-075",
            "[CII-DT-075] - SeasonCode shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:ID)
fn validate_cii_dt_076(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-076",
            "[CII-DT-076] - ID shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:Name)
fn validate_cii_dt_077(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-077",
            "[CII-DT-077] - Name shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:SequenceNumeric)
fn validate_cii_dt_078(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-078",
            "[CII-DT-078] - SequenceNumeric shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:StartDateFlexibilityCode)
fn validate_cii_dt_079(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-079",
            "[CII-DT-079] - StartDateFlexibilityCode shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:ContinuousIndicator)
fn validate_cii_dt_080(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-080",
            "[CII-DT-080] - ContinuousIndicator shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:BillingSpecifiedPeriod
// Test: not(ram:PurposeCode)
fn validate_cii_dt_081(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-081",
            "[CII-DT-081] - PurposeCode shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:ID)
fn validate_cii_dt_082(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-082",
            "[CII-DT-082] - ID shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:PostOfficeBox)
fn validate_cii_dt_083(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-083",
            "[CII-DT-083] - PostOfficeBox shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:BuildingName)
fn validate_cii_dt_084(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-084",
            "[CII-DT-084] - BuildingName shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:LineFour)
fn validate_cii_dt_086(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-086",
            "[CII-DT-086] - LineFour shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:LineFive)
fn validate_cii_dt_087(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-087",
            "[CII-DT-087] - LineFive shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:StreetName)
fn validate_cii_dt_088(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-088",
            "[CII-DT-088] - StreetName shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:CitySubDivisionName)
fn validate_cii_dt_089(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-089",
            "[CII-DT-089] - CitySubDivisionName shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:CountryName)
fn validate_cii_dt_090(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-090",
            "[CII-DT-090] - CountryName shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:CountrySubDivisionID)
fn validate_cii_dt_091(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-091",
            "[CII-DT-091] - CountrySubDivisionID shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:AttentionOf)
fn validate_cii_dt_092(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-092",
            "[CII-DT-092] - AttentionOf shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:CareOf)
fn validate_cii_dt_093(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-093",
            "[CII-DT-093] - CareOf shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:BuildingNumber)
fn validate_cii_dt_094(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-094",
            "[CII-DT-094] - BuildingNumber shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:DepartmentName)
fn validate_cii_dt_095(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-095",
            "[CII-DT-095] - DepartmentName shall not be used.",
        )));
    }
    Ok(())
}

// Context: //ram:PostalTradeAddress
// Test: not(ram:AdditionalStreetName)
fn validate_cii_dt_096(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-096",
            "[CII-DT-096] - AdditionalStreetName shall not be used.",
        )));
    }
    Ok(())
}

// Context: //udt:DateTimeString[@format = '102']
// Test: matches(.,'^\s*(\d{4})(1[0-2]|0[1-9]){1}(3[01]|[12][0-9]|0[1-9]){1}\s*$')
fn validate_cii_dt_097(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "CII-DT-097",
            "[CII-DT-097] - Date time string with format attribute 102 shall be YYYYMMDD.",
        )));
    }
    Ok(())
}

// Context: rsm:ExchangedDocument/ram:TypeCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' 71 80 81 82 83 84 102 130 202 203 204 211 218 219 261 262 295 296 308 325 326 331 380 381 382 383 384 385 386 387 388 389 390 393 394 395 396 420 456 457 458 527 532 553 575 623 633 751 780 817 870 875 876 877 935 ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_01(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-01",
            "[BR-CL-01]-The document type code MUST be coded by the invoice and credit note related code lists of UNTDID 1001.",
        )));
    }
    Ok(())
}

// Context: ram:TaxTotalAmount[@currencyID]
// Test: ((not(contains(normalize-space(@currencyID), ' ')) and contains(' AED AFN ALL AMD ANG AOA ARS AUD AWG AZN BAM BBD BDT BGN BHD BIF BMD BND BOB BOV BRL BSD BTN BWP BYN BZD CAD CDF CHE CHF CHW CLF CLP CNY COP COU CRC CUC CUP CVE CZK DJF DKK DOP DZD EGP ERN ETB EUR FJD FKP GBP GEL GHS GIP GMD GNF GTQ GYD HKD HNL HTG HUF IDR ILS INR IQD IRR ISK JMD JOD JPY KES KGS KHR KMF KPW KRW KWD KYD KZT LAK LBP LKR LRD LSL LYD MAD MDL MGA MKD MMK MNT MOP MRO MUR MVR MWK MXN MXV MYR MZN NAD NGN NIO NOK NPR NZD OMR PAB PEN PGK PHP PKR PLN PYG QAR RON RSD RUB RWF SAR SBD SCR SDG SEK SGD SHP SLE SOS SRD SSP STD SVC SYP SZL THB TJS TMT TND TOP TRY TTD TWD TZS UAH UGX USD USN UYI UYU UZS VES VED VND VUV WST XAF XAG XAU XBA XBB XBC XBD XCD XDR XOF XPD XPF XPT XSU XTS XUA XXX YER ZAR ZMW ZWG ZWL ', concat(' ', normalize-space(@currencyID), ' '))))
fn validate_br_cl_03(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-03",
            "[BR-CL-03]-currencyID MUST be coded using ISO code list 4217 alpha-3",
        )));
    }
    Ok(())
}

// Context: ram:InvoiceCurrencyCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' AED AFN ALL AMD ANG AOA ARS AUD AWG AZN BAM BBD BDT BGN BHD BIF BMD BND BOB BOV BRL BSD BTN BWP BYN BZD CAD CDF CHE CHF CHW CLF CLP CNY COP COU CRC CUC CUP CVE CZK DJF DKK DOP DZD EGP ERN ETB EUR FJD FKP GBP GEL GHS GIP GMD GNF GTQ GYD HKD HNL HTG HUF IDR ILS INR IQD IRR ISK JMD JOD JPY KES KGS KHR KMF KPW KRW KWD KYD KZT LAK LBP LKR LRD LSL LYD MAD MDL MGA MKD MMK MNT MOP MRO MUR MVR MWK MXN MXV MYR MZN NAD NGN NIO NOK NPR NZD OMR PAB PEN PGK PHP PKR PLN PYG QAR RON RSD RUB RWF SAR SBD SCR SDG SEK SGD SHP SLE SOS SRD SSP STD SVC SYP SZL THB TJS TMT TND TOP TRY TTD TWD TZS UAH UGX USD USN UYI UYU UZS VES VED VND VUV WST XAF XAG XAU XBA XBB XBC XBD XCD XDR XOF XPD XPF XPT XSU XTS XUA XXX YER ZAR ZMW ZWG ZWL ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_04(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-04",
            "[BR-CL-04]-Invoice currency code MUST be coded using ISO code list 4217 alpha-3",
        )));
    }
    Ok(())
}

// Context: ram:TaxCurrencyCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' AED AFN ALL AMD ANG AOA ARS AUD AWG AZN BAM BBD BDT BGN BHD BIF BMD BND BOB BOV BRL BSD BTN BWP BYN BZD CAD CDF CHE CHF CHW CLF CLP CNY COP COU CRC CUC CUP CVE CZK DJF DKK DOP DZD EGP ERN ETB EUR FJD FKP GBP GEL GHS GIP GMD GNF GTQ GYD HKD HNL HTG HUF IDR ILS INR IQD IRR ISK JMD JOD JPY KES KGS KHR KMF KPW KRW KWD KYD KZT LAK LBP LKR LRD LSL LYD MAD MDL MGA MKD MMK MNT MOP MRO MUR MVR MWK MXN MXV MYR MZN NAD NGN NIO NOK NPR NZD OMR PAB PEN PGK PHP PKR PLN PYG QAR RON RSD RUB RWF SAR SBD SCR SDG SEK SGD SHP SLE SOS SRD SSP STD SVC SYP SZL THB TJS TMT TND TOP TRY TTD TWD TZS UAH UGX USD USN UYI UYU UZS VES VED VND VUV WST XAF XAG XAU XBA XBB XBC XBD XCD XDR XOF XPD XPF XPT XSU XTS XUA XXX YER ZAR ZMW ZWG ZWL ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_05(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-05",
            "[BR-CL-05]-Tax currency code MUST be coded using ISO code list 4217 alpha-3",
        )));
    }
    Ok(())
}

// Context: ram:DueDateTypeCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' 5 29 72 ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_06(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-06",
            "[BR-CL-06]-Value added tax point date code MUST be coded using a restriction of UNTDID 2475.",
        )));
    }
    Ok(())
}

// Context: ram:ReferenceTypeCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' AAA AAB AAC AAD AAE AAF AAG AAH AAI AAJ AAK AAL AAM AAN AAO AAP AAQ AAR AAS AAT AAU AAV AAW AAX AAY AAZ ABA ABB ABC ABD ABE ABF ABG ABH ABI ABJ ABK ABL ABM ABN ABO ABP ABQ ABR ABS ABT ABU ABV ABW ABX ABY ABZ AC ACA ACB ACC ACD ACE ACF ACG ACH ACI ACJ ACK ACL ACN ACO ACP ACQ ACR ACT ACU ACV ACW ACX ACY ACZ ADA ADB ADC ADD ADE ADF ADG ADI ADJ ADK ADL ADM ADN ADO ADP ADQ ADT ADU ADV ADW ADX ADY ADZ AE AEA AEB AEC AED AEE AEF AEG AEH AEI AEJ AEK AEL AEM AEN AEO AEP AEQ AER AES AET AEU AEV AEW AEX AEY AEZ AF AFA AFB AFC AFD AFE AFF AFG AFH AFI AFJ AFK AFL AFM AFN AFO AFP AFQ AFR AFS AFT AFU AFV AFW AFX AFY AFZ AGA AGB AGC AGD AGE AGF AGG AGH AGI AGJ AGK AGL AGM AGN AGO AGP AGQ AGR AGS AGT AGU AGV AGW AGX AGY AGZ AHA AHB AHC AHD AHE AHF AHG AHH AHI AHJ AHK AHL AHM AHN AHO AHP AHQ AHR AHS AHT AHU AHV AHX AHY AHZ AIA AIB AIC AID AIE AIF AIG AIH AII AIJ AIK AIL AIM AIN AIO AIP AIQ AIR AIS AIT AIU AIV AIW AIX AIY AIZ AJA AJB AJC AJD AJE AJF AJG AJH AJI AJJ AJK AJL AJM AJN AJO AJP AJQ AJR AJS AJT AJU AJV AJW AJX AJY AJZ AKA AKB AKC AKD AKE AKF AKG AKH AKI AKJ AKK AKL AKM AKN AKO AKP AKQ AKR AKS AKT AKU AKV AKW AKX AKY AKZ ALA ALB ALC ALD ALE ALF ALG ALH ALI ALJ ALK ALL ALM ALN ALO ALP ALQ ALR ALS ALT ALU ALV ALW ALX ALY ALZ AMA AMB AMC AMD AME AMF AMG AMH AMI AMJ AMK AML AMM AMN AMO AMP AMQ AMR AMS AMT AMU AMV AMW AMX AMY AMZ ANA ANB ANC AND ANE ANF ANG ANH ANI ANJ ANK ANL ANM ANN ANO ANP ANQ ANR ANS ANT ANU ANV ANW ANX ANY AOA AOD AOE AOF AOG AOH AOI AOJ AOK AOL AOM AON AOO AOP AOQ AOR AOS AOT AOU AOV AOW AOX AOY AOZ AP APA APB APC APD APE APF APG APH API APJ APK APL APM APN APO APP APQ APR APS APT APU APV APW APX APY APZ AQA AQB AQC AQD AQE AQF AQG AQH AQI AQJ AQK AQL AQM AQN AQO AQP AQQ AQR AQS AQT AQU AQV AQW AQX AQY AQZ ARA ARB ARC ARD ARE ARF ARG ARH ARI ARJ ARK ARL ARM ARN ARO ARP ARQ ARR ARS ART ARU ARV ARW ARX ARY ARZ ASA ASB ASC ASD ASE ASF ASG ASH ASI ASJ ASK ASL ASM ASN ASO ASP ASQ ASR ASS AST ASU ASV ASW ASX ASY ASZ ATA ATB ATC ATD ATE ATF ATG ATH ATI ATJ ATK ATL ATM ATN ATO ATP ATQ ATR ATS ATT ATU ATV ATW ATX ATY ATZ AU AUA AUB AUC AUD AUE AUF AUG AUH AUI AUJ AUK AUL AUM AUN AUO AUP AUQ AUR AUS AUT AUU AUV AUW AUX AUY AUZ AV AVA AVB AVC AVD AVE AVF AVG AVH AVI AVJ AVK AVL AVM AVN AVO AVP AVQ AVR AVS AVT AVU AVV AVW AVX AVY AVZ AWA AWB AWC AWD AWE AWF AWG AWH AWI AWJ AWK AWL AWM AWN AWO AWP AWQ AWR AWS AWT AWU AWV AWW AWX AWY AWZ AXA AXB AXC AXD AXE AXF AXG AXH AXI AXJ AXK AXL AXM AXN AXO AXP AXQ AXR AXS BA BC BD BE BH BM BN BO BR BT BTP BW CAS CAT CAU CAV CAW CAX CAY CAZ CBA CBB CD CEC CED CFE CFF CFO CG CH CK CKN CM CMR CN CNO COF CP CR CRN CS CST CT CU CV CW CZ DA DAN DB DI DL DM DQ DR EA EB ED EE EEP EI EN EQ ER ERN ET EX FC FF FI FLW FN FO FS FT FV FX GA GC GD GDN GN HS HWB IA IB ICA ICE ICO II IL INB INN INO IP IS IT IV JB JE LA LAN LAR LB LC LI LO LRC LS MA MB MF MG MH MR MRN MS MSS MWB NA NF OH OI ON OP OR PB PC PD PE PF PI PK PL POR PP PQ PR PS PW PY RA RC RCN RE REN RF RR RT SA SB SD SE SEA SF SH SI SM SN SP SQ SRN SS STA SW SZ TB TCR TE TF TI TIN TL TN TP UAR UC UCN UN UO URI VA VC VGR VM VN VON VOR VP VR VS VT VV WE WM WN WR WS WY XA XC XP ZZZ ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_07(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-07",
            "[BR-CL-07]-Object identifier identification scheme identifier MUST be coded using a restriction of UNTDID 1153.",
        )));
    }
    Ok(())
}

// Context: ram:SubjectCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' AAA AAB AAC AAD AAE AAF AAG AAI AAJ AAK AAL AAM AAN AAO AAP AAQ AAR AAS AAT AAU AAV AAW AAX AAY AAZ ABA ABB ABC ABD ABE ABF ABG ABH ABI ABJ ABK ABL ABM ABN ABO ABP ABQ ABR ABS ABT ABU ABV ABW ABX ABZ ACA ACB ACC ACD ACE ACF ACG ACH ACI ACJ ACK ACL ACM ACN ACO ACP ACQ ACR ACS ACT ACU ACV ACW ACX ACY ACZ ADA ADB ADC ADD ADE ADF ADG ADH ADI ADJ ADK ADL ADM ADN ADO ADP ADQ ADR ADS ADT ADU ADV ADW ADX ADY ADZ AEA AEB AEC AED AEE AEF AEG AEH AEI AEJ AEK AEL AEM AEN AEO AEP AEQ AER AES AET AEU AEV AEW AEX AEY AEZ AFA AFB AFC AFD AFE AFF AFG AFH AFI AFJ AFK AFL AFM AFN AFO AFP AFQ AFR AFS AFT AFU AFV AFW AFX AFY AFZ AGA AGB AGC AGD AGE AGF AGG AGH AGI AGJ AGK AGL AGM AGN AGO AGP AGQ AGR AGS AGT AGU AGV AGW AGX AGY AGZ AHA AHB AHC AHD AHE AHF AHG AHH AHI AHJ AHK AHL AHM AHN AHO AHP AHQ AHR AHS AHT AHU AHV AHW AHX AHY AHZ AIA AIB AIC AID AIE AIF AIG AIH AII AIJ AIK AIL AIM AIN AIO AIP AIQ AIR AIS AIT AIU AIV AIW AIX AIY AIZ AJA AJB ALC ALD ALE ALF ALG ALH ALI ALJ ALK ALL ALM ALN ALO ALP ALQ ARR ARS AUT AUU AUV AUW AUX AUY AUZ AVA AVB AVC AVD AVE AVF BAG BAH BAI BAJ BAK BAL BAM BAN BAO BAP BAQ BAR BAS BAT BAU BAV BAW BAX BAY BAZ BBA BBB BLC BLD BLE BLF BLG BLH BLI BLJ BLK BLL BLM BLN BLO BLP BLQ BLR BLS BLT BLU BLV BLW BLX BLY BLZ BMA BMB BMC BMD BME BMF BMG BMH CCI CCJ CCK CCL CCM CCN CCO CEX CHG CIP CLP CLR COI CUR CUS DAR DCL DEL DIN DOC DUT EUR FBC GBL GEN GS7 HAN HAZ ICN IIN IMI IND INS INV IRP ITR ITS LAN LIN LOI MCO MDH MKS ORI OSI PAC PAI PAY PKG PKT PMD PMT PRD PRF PRI PUR QIN QQD QUT RAH REG RET REV RQR SAF SIC SIN SLR SPA SPG SPH SPP SPT SRN SSR SUR TCA TDT TRA TRR TXD WHI ZZZ ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_08(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-08",
            "[BR-CL-08]-Subject Code MUST be coded using a restriction of UNTDID 4451.",
        )));
    }
    Ok(())
}

// Context: //ram:GlobalID[@schemeID][not(ancestor::ram:SpecifiedTradeProduct) and not(ancestor::ram:ShipToTradeParty)]
// Test: ((not(contains(normalize-space(@schemeID), ' ')) and contains(' 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048 0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0093 0094 0095 0096 0097 0098 0099 0100 0101 0102 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144 0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192 0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 ', concat(' ', normalize-space(@schemeID), ' '))))
fn validate_br_cl_10(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-10",
            "[BR-CL-10]-Any identifier identification scheme identifier MUST be coded using one of the ISO 6523 ICD list.",
        )));
    }
    Ok(())
}

// Context: ram:ID[@schemeID][not(ancestor::ram:SpecifiedTaxRegistration)]
// Test: ((not(contains(normalize-space(@schemeID), ' ')) and contains(' 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048 0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0093 0094 0095 0096 0097 0098 0099 0100 0101 0102 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144 0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192 0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 ', concat(' ', normalize-space(@schemeID), ' '))))
fn validate_br_cl_11(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-11",
            "[BR-CL-11]-Any registration identifier identification scheme identifier MUST be coded using one of the ISO 6523 ICD list.",
        )));
    }
    Ok(())
}

// Context: ram:ClassCode[@listID]
// Test: ((not(contains(normalize-space(@listID), ' ')) and contains(' AA AB AC AD AE AF AG AH AI AJ AK AL AM AN AO AP AQ AR AS AT AU AV AW AX AY AZ BA BB BC BD BE BF BG BH BI BJ BK BL BM BN BO BP BQ BR BS BT BU BV BW BX BY BZ CC CG CL CR CV DR DW EC EF EMD EN FS GB GN GMN GS HS IB IN IS IT IZ MA MF MN MP NB ON PD PL PO PV QS RC RN RU RY SA SG SK SN SRS SRT SRU SRV SRW SRX SRY SRZ SS SSA SSB SSC SSD SSE SSF SSG SSH SSI SSJ SSK SSL SSM SSN SSO SSP SSQ SSR SSS SST SSU SSV SSW SSX SSY SSZ ST STA STB STC STD STE STF STG STH STI STJ STK STL STM STN STO STP STQ STR STS STT STU STV STW STX STY STZ SUA SUB SUC SUD SUE SUF SUG SUH SUI SUJ SUK SUL SUM TG TSN TSO TSP TSQ TSR TSS TST TSU UA UP VN VP VS VX ZZZ ', concat(' ', normalize-space(@listID), ' '))))
fn validate_br_cl_13(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-13",
            "[BR-CL-13]-Item classification identifier identification scheme identifier MUST be coded using one of the UNTDID 7143 list.",
        )));
    }
    Ok(())
}

// Context: ram:CountryID
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' 1A AD AE AF AG AI AL AM AN AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BL BJ BM BN BO BQ BR BS BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS XI YE YT ZA ZM ZW ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_14(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-14",
            "[BR-CL-14]-Country codes in an invoice MUST be coded using ISO code list 3166-1",
        )));
    }
    Ok(())
}

// Context: ram:OriginTradeCountry/ram:ID
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' 1A AD AE AF AG AI AL AM AN AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BL BJ BM BN BO BQ BR BS BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS XI YE YT ZA ZM ZW ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_15(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-15",
            "[BR-CL-15]-Country codes in an invoice MUST be coded using ISO code list 3166-1",
        )));
    }
    Ok(())
}

// Context: ram:SpecifiedTradeSettlementPaymentMeans/ram:TypeCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63 64 65 66 67 68 69 70 74 75 76 77 78 91 92 93 94 95 96 97 98 ZZZ ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_16(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-16",
            "[BR-CL-16]-Payment means in an invoice MUST be coded using UNTDID 4461 code list",
        )));
    }
    Ok(())
}

// Context: ram:CategoryTradeTax/ram:CategoryCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' AE L M E S Z G O K B ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_17(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-17",
            "[BR-CL-17]-Invoice tax categories MUST be coded using UNCL 5305 code list",
        )));
    }
    Ok(())
}

// Context: ram:ApplicableTradeTax/ram:CategoryCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' AE L M E S Z G O K B ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_18(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-18",
            "[BR-CL-18]-Invoice tax categories MUST be coded using UNCL 5305 code list",
        )));
    }
    Ok(())
}

// Context: ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator = false()]/ram:ReasonCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' 41 42 60 62 63 64 65 66 67 68 70 71 88 95 100 102 103 104 105 ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_19(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-19",
            "[BR-CL-19]-Coded allowance reasons MUST belong to the UNCL 5189 code list",
        )));
    }
    Ok(())
}

// Context: ram:SpecifiedTradeAllowanceCharge[ram:ChargeIndicator/udt:Indicator = true()]/ram:ReasonCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' AA AAA AAC AAD AAE AAF AAH AAI AAS AAT AAV AAY AAZ ABA ABB ABC ABD ABF ABK ABL ABN ABR ABS ABT ABU ACF ACG ACH ACI ACJ ACK ACL ACM ACS ADC ADE ADJ ADK ADL ADM ADN ADO ADP ADQ ADR ADT ADW ADY ADZ AEA AEB AEC AED AEF AEH AEI AEJ AEK AEL AEM AEN AEO AEP AES AET AEU AEV AEW AEX AEY AEZ AJ AU CA CAB CAD CAE CAF CAI CAJ CAK CAL CAM CAN CAO CAP CAQ CAR CAS CAT CAU CAV CAW CAX CAY CAZ CD CG CS CT DAB DAD DAC DAF DAG DAH DAI DAJ DAK DAL DAM DAN DAO DAP DAQ DL EG EP ER FAA FAB FAC FC FH FI GAA HAA HD HH IAA IAB ID IF IR IS KO L1 LA LAA LAB LF MAE MI ML NAA OA PA PAA PC PL PRV RAB RAC RAD RAF RE RF RH RV SA SAA SAD SAE SAI SG SH SM SU TAB TAC TT TV V1 V2 WH XAA YY ZZZ ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_20(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-20",
            "[BR-CL-20]-Coded charge reasons MUST belong to the UNCL 7161 code list",
        )));
    }
    Ok(())
}

// Context: ram:SpecifiedTradeProduct/ram:GlobalID[@schemeID]
// Test: ((not(contains(normalize-space(@schemeID), ' ')) and contains(' 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048 0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0093 0094 0095 0096 0097 0098 0099 0100 0101 0102 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144 0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192 0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 ', concat(' ', normalize-space(@schemeID), ' '))))
fn validate_br_cl_21(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-21",
            "[BR-CL-21]-Item standard identifier scheme identifier MUST belong to the ISO 6523 ICD
      code list",
        )));
    }
    Ok(())
}

// Context: ram:ExemptionReasonCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' VATEX-EU-79-C VATEX-EU-132 VATEX-EU-132-1A VATEX-EU-132-1B VATEX-EU-132-1C VATEX-EU-132-1D VATEX-EU-132-1E VATEX-EU-132-1F VATEX-EU-132-1G VATEX-EU-132-1H VATEX-EU-132-1I VATEX-EU-132-1J VATEX-EU-132-1K VATEX-EU-132-1L VATEX-EU-132-1M VATEX-EU-132-1N VATEX-EU-132-1O VATEX-EU-132-1P VATEX-EU-132-1Q VATEX-EU-143 VATEX-EU-143-1A VATEX-EU-143-1B VATEX-EU-143-1C VATEX-EU-143-1D VATEX-EU-143-1E VATEX-EU-143-1F VATEX-EU-143-1FA VATEX-EU-143-1G VATEX-EU-143-1H VATEX-EU-143-1I VATEX-EU-143-1J VATEX-EU-143-1K VATEX-EU-143-1L VATEX-EU-144 VATEX-EU-146-1E VATEX-EU-159 VATEX-EU-309 VATEX-EU-148 VATEX-EU-148-A VATEX-EU-148-B VATEX-EU-148-C VATEX-EU-148-D VATEX-EU-148-E VATEX-EU-148-F VATEX-EU-148-G VATEX-EU-151 VATEX-EU-151-1A VATEX-EU-151-1AA VATEX-EU-151-1B VATEX-EU-151-1C VATEX-EU-151-1D VATEX-EU-151-1E VATEX-EU-G VATEX-EU-O VATEX-EU-IC VATEX-EU-AE VATEX-EU-D VATEX-EU-F VATEX-EU-I VATEX-EU-J VATEX-FR-FRANCHISE VATEX-FR-CNWVAT ', concat(' ', normalize-space(upper-case(.)), ' '))))
fn validate_br_cl_22(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-22",
            "[BR-CL-22]-Tax exemption reason code identifier scheme identifier MUST belong to the CEF VATEX code list",
        )));
    }
    Ok(())
}

// Context: ram:BasisQuantity[@unitCode] | ram:BilledQuantity[@unitCode]
// Test: ((not(contains(normalize-space(@unitCode), ' ')) and contains(' 10 11 13 14 15 20 21 22 23 24 25 27 28 33 34 35 37 38 40 41 56 57 58 59 60 61 74 77 80 81 85 87 89 91 1I 2A 2B 2C 2G 2H 2I 2J 2K 2L 2M 2N 2P 2Q 2R 2U 2X 2Y 2Z 3B 3C 4C 4G 4H 4K 4L 4M 4N 4O 4P 4Q 4R 4T 4U 4W 4X 5A 5B 5E 5J A10 A11 A12 A13 A14 A15 A16 A17 A18 A19 A2 A20 A21 A22 A23 A24 A26 A27 A28 A29 A3 A30 A31 A32 A33 A34 A35 A36 A37 A38 A39 A4 A40 A41 A42 A43 A44 A45 A47 A48 A49 A5 A53 A54 A55 A56 A59 A6 A68 A69 A7 A70 A71 A73 A74 A75 A76 A8 A84 A85 A86 A87 A88 A89 A9 A90 A91 A93 A94 A95 A96 A97 A98 A99 AA AB ACR ACT AD AE AH AI AK AL AMH AMP ANN APZ AQ AS ASM ASU ATM AWG AY AZ B1 B10 B11 B12 B13 B14 B15 B16 B17 B18 B19 B20 B21 B22 B23 B24 B25 B26 B27 B28 B29 B3 B30 B31 B32 B33 B34 B35 B4 B41 B42 B43 B44 B45 B46 B47 B48 B49 B50 B52 B53 B54 B55 B56 B57 B58 B59 B60 B61 B62 B63 B64 B66 B67 B68 B69 B7 B70 B71 B72 B73 B74 B75 B76 B77 B78 B79 B8 B80 B81 B82 B83 B84 B85 B86 B87 B88 B89 B90 B91 B92 B93 B94 B95 B96 B97 B98 B99 BAR BB BFT BHP BIL BLD BLL BP BPM BQL BTU BUA BUI C0 C10 C11 C12 C13 C14 C15 C16 C17 C18 C19 C20 C21 C22 C23 C24 C25 C26 C27 C28 C29 C3 C30 C31 C32 C33 C34 C35 C36 C37 C38 C39 C40 C41 C42 C43 C44 C45 C46 C47 C48 C49 C50 C51 C52 C53 C54 C55 C56 C57 C58 C59 C60 C61 C62 C63 C64 C65 C66 C67 C68 C69 C7 C70 C71 C72 C73 C74 C75 C76 C78 C79 C8 C80 C81 C82 C83 C84 C85 C86 C87 C88 C89 C9 C90 C91 C92 C93 C94 C95 C96 C97 C99 CCT CDL CEL CEN CG CGM CKG CLF CLT CMK CMQ CMT CNP CNT COU CTG CTM CTN CUR CWA CWI D03 D04 D1 D10 D11 D12 D13 D15 D16 D17 D18 D19 D2 D20 D21 D22 D23 D24 D25 D26 D27 D29 D30 D31 D32 D33 D34 D36 D41 D42 D43 D44 D45 D46 D47 D48 D49 D5 D50 D51 D52 D53 D54 D55 D56 D57 D58 D59 D6 D60 D61 D62 D63 D65 D68 D69 D73 D74 D77 D78 D80 D81 D82 D83 D85 D86 D87 D88 D89 D91 D93 D94 D95 DAA DAD DAY DB DBM DBW DD DEC DG DJ DLT DMA DMK DMO DMQ DMT DN DPC DPR DPT DRA DRI DRL DT DTN DWT DZN DZP E01 E07 E08 E09 E10 E12 E14 E15 E16 E17 E18 E19 E20 E21 E22 E23 E25 E27 E28 E30 E31 E32 E33 E34 E35 E36 E37 E38 E39 E4 E40 E41 E42 E43 E44 E45 E46 E47 E48 E49 E50 E51 E52 E53 E54 E55 E56 E57 E58 E59 E60 E61 E62 E63 E64 E65 E66 E67 E68 E69 E70 E71 E72 E73 E74 E75 E76 E77 E78 E79 E80 E81 E82 E83 E84 E85 E86 E87 E88 E89 E90 E91 E92 E93 E94 E95 E96 E97 E98 E99 EA EB EQ F01 F02 F03 F04 F05 F06 F07 F08 F10 F11 F12 F13 F14 F15 F16 F17 F18 F19 F20 F21 F22 F23 F24 F25 F26 F27 F28 F29 F30 F31 F32 F33 F34 F35 F36 F37 F38 F39 F40 F41 F42 F43 F44 F45 F46 F47 F48 F49 F50 F51 F52 F53 F54 F55 F56 F57 F58 F59 F60 F61 F62 F63 F64 F65 F66 F67 F68 F69 F70 F71 F72 F73 F74 F75 F76 F77 F78 F79 F80 F81 F82 F83 F84 F85 F86 F87 F88 F89 F90 F91 F92 F93 F94 F95 F96 F97 F98 F99 FAH FAR FBM FC FF FH FIT FL FNU FOT FP FR FS FTK FTQ G01 G04 G05 G06 G08 G09 G10 G11 G12 G13 G14 G15 G16 G17 G18 G19 G2 G20 G21 G23 G24 G25 G26 G27 G28 G29 G3 G30 G31 G32 G33 G34 G35 G36 G37 G38 G39 G40 G41 G42 G43 G44 G45 G46 G47 G48 G49 G50 G51 G52 G53 G54 G55 G56 G57 G58 G59 G60 G61 G62 G63 G64 G65 G66 G67 G68 G69 G70 G71 G72 G73 G74 G75 G76 G77 G78 G79 G80 G81 G82 G83 G84 G85 G86 G87 G88 G89 G90 G91 G92 G93 G94 G95 G96 G97 G98 G99 GB GBQ GDW GE GF GFI GGR GIA GIC GII GIP GJ GL GLD GLI GLL GM GO GP GQ GRM GRN GRO GV GWH H03 H04 H05 H06 H07 H08 H09 H10 H11 H12 H13 H14 H15 H16 H18 H19 H20 H21 H22 H23 H24 H25 H26 H27 H28 H29 H30 H31 H32 H33 H34 H35 H36 H37 H38 H39 H40 H41 H42 H43 H44 H45 H46 H47 H48 H49 H50 H51 H52 H53 H54 H55 H56 H57 H58 H59 H60 H61 H62 H63 H64 H65 H66 H67 H68 H69 H70 H71 H72 H73 H74 H75 H76 H77 H79 H80 H81 H82 H83 H84 H85 H87 H88 H89 H90 H91 H92 H93 H94 H95 H96 H98 H99 HA HAD HBA HBX HC HDW HEA HGM HH HIU HKM HLT HM HMO HMQ HMT HPA HTZ HUR HWE IA IE INH INK INQ ISD IU IUG IV J10 J12 J13 J14 J15 J16 J17 J18 J19 J2 J20 J21 J22 J23 J24 J25 J26 J27 J28 J29 J30 J31 J32 J33 J34 J35 J36 J38 J39 J40 J41 J42 J43 J44 J45 J46 J47 J48 J49 J50 J51 J52 J53 J54 J55 J56 J57 J58 J59 J60 J61 J62 J63 J64 J65 J66 J67 J68 J69 J70 J71 J72 J73 J74 J75 J76 J78 J79 J81 J82 J83 J84 J85 J87 J90 J91 J92 J93 J95 J96 J97 J98 J99 JE JK JM JNT JOU JPS JWL K1 K10 K11 K12 K13 K14 K15 K16 K17 K18 K19 K2 K20 K21 K22 K23 K26 K27 K28 K3 K30 K31 K32 K33 K34 K35 K36 K37 K38 K39 K40 K41 K42 K43 K45 K46 K47 K48 K49 K50 K51 K52 K53 K54 K55 K58 K59 K6 K60 K61 K62 K63 K64 K65 K66 K67 K68 K69 K70 K71 K73 K74 K75 K76 K77 K78 K79 K80 K81 K82 K83 K84 K85 K86 K87 K88 K89 K90 K91 K92 K93 K94 K95 K96 K97 K98 K99 KA KAT KB KBA KCC KDW KEL KGM KGS KHY KHZ KI KIC KIP KJ KJO KL KLK KLX KMA KMH KMK KMQ KMT KNI KNM KNS KNT KO KPA KPH KPO KPP KR KSD KSH KT KTN KUR KVA KVR KVT KW KWH KWN KWO KWS KWT KWY KX L10 L11 L12 L13 L14 L15 L16 L17 L18 L19 L2 L20 L21 L23 L24 L25 L26 L27 L28 L29 L30 L31 L32 L33 L34 L35 L36 L37 L38 L39 L40 L41 L42 L43 L44 L45 L46 L47 L48 L49 L50 L51 L52 L53 L54 L55 L56 L57 L58 L59 L60 L63 L64 L65 L66 L67 L68 L69 L70 L71 L72 L73 L74 L75 L76 L77 L78 L79 L80 L81 L82 L83 L84 L85 L86 L87 L88 L89 L90 L91 L92 L93 L94 L95 L96 L98 L99 LA LAC LBR LBT LD LEF LF LH LK LM LN LO LP LPA LR LS LTN LTR LUB LUM LUX LY M1 M10 M11 M12 M13 M14 M15 M16 M17 M18 M19 M20 M21 M22 M23 M24 M25 M26 M27 M29 M30 M31 M32 M33 M34 M35 M36 M37 M38 M39 M4 M40 M41 M42 M43 M44 M45 M46 M47 M48 M49 M5 M50 M51 M52 M53 M55 M56 M57 M58 M59 M60 M61 M62 M63 M64 M65 M66 M67 M68 M69 M7 M70 M71 M72 M73 M74 M75 M76 M77 M78 M79 M80 M81 M82 M83 M84 M85 M86 M87 M88 M89 M9 M90 M91 M92 M93 M94 M95 M96 M97 M98 M99 MAH MAL MAM MAR MAW MBE MBF MBR MC MCU MD MGM MHZ MIK MIL MIN MIO MIU MKD MKM MKW MLD MLT MMK MMQ MMT MND MNJ MON MPA MQD MQH MQM MQS MQW MRD MRM MRW MSK MTK MTQ MTR MTS MTZ MVA MWH N1 N10 N11 N12 N13 N14 N15 N16 N17 N18 N19 N20 N21 N22 N23 N24 N25 N26 N27 N28 N29 N3 N30 N31 N32 N33 N34 N35 N36 N37 N38 N39 N40 N41 N42 N43 N44 N45 N46 N47 N48 N49 N50 N51 N52 N53 N54 N55 N56 N57 N58 N59 N60 N61 N62 N63 N64 N65 N66 N67 N68 N69 N70 N71 N72 N73 N74 N75 N76 N77 N78 N79 N80 N81 N82 N83 N84 N85 N86 N87 N88 N89 N90 N91 N92 N93 N94 N95 N96 N97 N98 N99 NA NAR NCL NEW NF NIL NIU NL NM3 NMI NMP NPT NT NTU NU NX OA ODE ODG ODK ODM OHM ON ONZ OPM OT OZA OZI P1 P10 P11 P12 P13 P14 P15 P16 P17 P18 P19 P2 P20 P21 P22 P23 P24 P25 P26 P27 P28 P29 P30 P31 P32 P33 P34 P35 P36 P37 P38 P39 P40 P41 P42 P43 P44 P45 P46 P47 P48 P49 P5 P50 P51 P52 P53 P54 P55 P56 P57 P58 P59 P60 P61 P62 P63 P64 P65 P66 P67 P68 P69 P70 P71 P72 P73 P74 P75 P76 P77 P78 P79 P80 P81 P82 P83 P84 P85 P86 P87 P88 P89 P90 P91 P92 P93 P94 P95 P96 P97 P98 P99 PAL PD PFL PGL PI PLA PO PQ PR PS PTD PTI PTL PTN Q10 Q11 Q12 Q13 Q14 Q15 Q16 Q17 Q18 Q19 Q20 Q21 Q22 Q23 Q24 Q25 Q26 Q27 Q28 Q29 Q3 Q30 Q31 Q32 Q33 Q34 Q35 Q36 Q37 Q38 Q39 Q40 Q41 Q42 QA QAN QB QR QTD QTI QTL QTR R1 R9 RH RM ROM RP RPM RPS RT S3 S4 SAN SCO SCR SEC SET SG SIE SM3 SMI SQ SQR SR STC STI STK STL STN STW SW SX SYR T0 T3 TAH TAN TI TIC TIP TKM TMS TNE TP TPI TPR TQD TRL TST TTS U1 U2 UB UC VA VLT VP W2 WA WB WCD WE WEB WEE WG WHR WM WSD WTT X1 YDK YDQ YRD Z11 Z9 ZP ZZ X1A X1B X1D X1F X1G X1W X2C X3A X3H X43 X44 X4A X4B X4C X4D X4F X4G X4H X5H X5L X5M X6H X6P X7A X7B X8A X8B X8C XAA XAB XAC XAD XAE XAF XAG XAH XAI XAJ XAL XAM XAP XAT XAV XB4 XBA XBB XBC XBD XBE XBF XBG XBH XBI XBJ XBK XBL XBM XBN XBO XBP XBQ XBR XBS XBT XBU XBV XBW XBX XBY XBZ XCA XCB XCC XCD XCE XCF XCG XCH XCI XCJ XCK XCL XCM XCN XCO XCP XCQ XCR XCS XCT XCU XCV XCW XCX XCY XCZ XDA XDB XDC XDG XDH XDI XDJ XDK XDL XDM XDN XDP XDR XDS XDT XDU XDV XDW XDX XDY XEC XED XEE XEF XEG XEH XEI XEN XFB XFC XFD XFE XFI XFL XFO XFP XFR XFT XFW XFX XGB XGI XGL XGR XGU XGY XGZ XHA XHB XHC XHG XHN XHR XIA XIB XIC XID XIE XIF XIG XIH XIK XIL XIN XIZ XJB XJC XJG XJR XJT XJY XKG XKI XLE XLG XLT XLU XLV XLZ XMA XMB XMC XME XMR XMS XMT XMW XMX XNA XNE XNF XNG XNS XNT XNU XNV XO1 XO2 XO3 XO4 XO5 XO6 XO7 XO8 XO9 XOA XOB XOC XOD XOE XOF XOG XOH XOI XOJ XOK XOL XOM XON XOP XOQ XOR XOS XOT XOU XOV XOW XOX XOY XOZ XP1 XP2 XP3 XP4 XPA XPB XPC XPD XPE XPF XPG XPH XPI XPJ XPK XPL XPN XPO XPP XPR XPT XPU XPV XPX XPY XPZ XQA XQB XQC XQD XQF XQG XQH XQJ XQK XQL XQM XQN XQP XQQ XQR XQS XRD XRG XRJ XRK XRL XRO XRT XRZ XSA XSB XSC XSD XSE XSH XSI XSK XSL XSM XSO XSP XSS XST XSU XSV XSW XSX XSY XSZ XT1 XTB XTC XTD XTE XTG XTI XTK XTL XTN XTO XTR XTS XTT XTU XTV XTW XTY XTZ XUC XUN XVA XVG XVI XVK XVL XVN XVO XVP XVQ XVR XVS XVY XWA XWB XWC XWD XWF XWG XWH XWJ XWK XWL XWM XWN XWP XWQ XWR XWS XWT XWU XWV XWW XWX XWY XWZ XXA XXB XXC XXD XXF XXG XXH XXJ XXK XYA XYB XYC XYD XYF XYG XYH XYJ XYK XYL XYM XYN XYP XYQ XYR XYS XYT XYV XYW XYX XYY XYZ XZA XZB XZC XZD XZF XZG XZH XZJ XZK XZL XZM XZN XZP XZQ XZR XZS XZT XZU XZV XZW XZX XZY XZZ ', concat(' ', normalize-space(@unitCode), ' '))))
fn validate_br_cl_23(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-23",
            "[BR-CL-23]-Unit code MUST be coded according to the UN/ECE Recommendation 20 with Rec 21 extension",
        )));
    }
    Ok(())
}

// Context: ram:AttachmentBinaryObject[@mimeCode]
// Test: ((@mimeCode = 'application/pdf' or @mimeCode  = 'image/png' or @mimeCode  = 'image/jpeg' or @mimeCode  = 'text/csv' or @mimeCode  = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' or @mimeCode  = 'application/vnd.oasis.opendocument.spreadsheet'))
fn validate_br_cl_24(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-24",
            "[BR-CL-24]-For Mime code in attribute use MIMEMediaType.",
        )));
    }
    Ok(())
}

// Context: ram:URIUniversalCommunication/ram:URIID[@schemeID]
// Test: ((not(contains(normalize-space(@schemeID), ' ')) and contains(' 0002 0007 0009 0037 0060 0088 0096 0097 0106 0130 0135 0142 0147 0151 0170 0177 0183 0184 0188 0190 0191 0192 0193 0194 0195 0196 0198 0199 0200 0201 0202 0203 0204 0205 0208 0209 0210 0211 0212 0213 0215 0216 0217 0218 0219 0220 0221 0225 0230 0235 9901 9910 9913 9914 9915 9918 9919 9920 9922 9923 9924 9925 9926 9927 9928 9929 9930 9931 9932 9933 9934 9935 9936 9937 9938 9939 9940 9941 9942 9943 9944 9945 9946 9947 9948 9949 9950 9951 9952 9953 9957 9959 AN AQ AS AU EM ', concat(' ', normalize-space(@schemeID), ' '))))
fn validate_br_cl_25(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-25",
            "[BR-CL-25]-Endpoint identifier scheme identifier MUST belong to the CEF EAS code list",
        )));
    }
    Ok(())
}

// Context: ram:ApplicableHeaderTradeDelivery/ram:ShipToTradeParty/ram:GlobalID[@schemeID]
// Test: ((not(contains(normalize-space(@schemeID), ' ')) and contains(' 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048 0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0093 0094 0095 0096 0097 0098 0099 0100 0101 0102 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144 0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192 0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 ', concat(' ', normalize-space(@schemeID), ' '))))
fn validate_br_cl_26(_invoice: &CrossIndustryInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-26",
            "[BR-CL-26]-Delivery location identifier scheme identifier MUST belong to the ISO 6523 ICD
      code list",
        )));
    }
    Ok(())
}
