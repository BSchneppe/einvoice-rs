use crate::{BusinessRuleViolation, UblInvoice, ValidationError};
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use std::sync::Arc;

pub fn validate_invoice(invoice: Arc<&UblInvoice>) -> Result<(), Vec<ValidationError>> {
    let results: Vec<_> = BUSINESS_RULES
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

type BusinessRule = fn(&UblInvoice) -> Result<(), ValidationError>;
pub const BUSINESS_RULES: &[BusinessRule] = &[
    validate_br_52,
    validate_br_co_25,
    validate_br_63,
    validate_br_11,
    validate_br_51,
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
    validate_br_10,
    validate_br_16,
    validate_br_53,
    validate_br_66,
    validate_br_67,
    validate_br_ae_01,
    validate_br_ae_02,
    validate_br_ae_03,
    validate_br_ae_04,
    validate_br_co_03,
    validate_br_co_15,
    validate_br_co_18,
    validate_br_dec_13,
    validate_br_dec_15,
    validate_br_e_01,
    validate_br_e_02,
    validate_br_e_03,
    validate_br_e_04,
    validate_br_g_01,
    validate_br_g_02,
    validate_br_g_03,
    validate_br_g_04,
    validate_br_ic_01,
    validate_br_ic_02,
    validate_br_ic_03,
    validate_br_ic_04,
    validate_br_ic_11,
    validate_br_ic_12,
    validate_br_ig_01,
    validate_br_ig_02,
    validate_br_ig_03,
    validate_br_ig_04,
    validate_br_ip_01,
    validate_br_ip_02,
    validate_br_ip_03,
    validate_br_ip_04,
    validate_br_o_01,
    validate_br_o_02,
    validate_br_o_03,
    validate_br_o_04,
    validate_br_o_11,
    validate_br_o_12,
    validate_br_o_13,
    validate_br_o_14,
    validate_br_s_01,
    validate_br_s_02,
    validate_br_s_03,
    validate_br_s_04,
    validate_br_z_01,
    validate_br_z_02,
    validate_br_z_03,
    validate_br_z_04,
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
    validate_br_co_04,
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
    validate_br_65,
    validate_br_64,
    validate_br_cl_08,
    validate_br_17,
    validate_br_50,
    validate_br_49,
    validate_br_61,
    validate_br_55,
    validate_br_co_26,
    validate_br_62,
    validate_br_09,
    validate_br_18,
    validate_br_19,
    validate_br_56,
    validate_br_20,
    validate_br_co_14,
    validate_br_45,
    validate_br_46,
    validate_br_47,
    validate_br_48,
    validate_br_co_17,
    validate_br_dec_19,
    validate_br_dec_20,
    validate_br_co_09,
    validate_br_ae_08,
    validate_br_ae_09,
    validate_br_ae_10,
    validate_br_ae_06,
    validate_br_ae_07,
    validate_br_ae_05,
    validate_br_e_08,
    validate_br_e_09,
    validate_br_e_10,
    validate_br_e_06,
    validate_br_e_07,
    validate_br_e_05,
    validate_br_g_08,
    validate_br_g_09,
    validate_br_g_10,
    validate_br_g_06,
    validate_br_g_07,
    validate_br_g_05,
    validate_br_ic_08,
    validate_br_ic_09,
    validate_br_ic_10,
    validate_br_ic_06,
    validate_br_ic_07,
    validate_br_ic_05,
    validate_br_ig_08,
    validate_br_ig_09,
    validate_br_ig_10,
    validate_br_ig_06,
    validate_br_ig_07,
    validate_br_ig_05,
    validate_br_ip_08,
    validate_br_ip_09,
    validate_br_ip_10,
    validate_br_ip_06,
    validate_br_ip_07,
    validate_br_ip_05,
    validate_br_o_08,
    validate_br_o_09,
    validate_br_o_10,
    validate_br_o_06,
    validate_br_o_07,
    validate_br_o_05,
    validate_br_s_08,
    validate_br_s_09,
    validate_br_s_10,
    validate_br_s_06,
    validate_br_s_07,
    validate_br_s_05,
    validate_br_z_08,
    validate_br_z_09,
    validate_br_z_10,
    validate_br_z_06,
    validate_br_z_07,
    validate_br_z_05,
    validate_ubl_sr_51,
    validate_ubl_sr_42,
    validate_ubl_sr_33,
    validate_ubl_sr_43,
    validate_ubl_dt_01,
    validate_ubl_dt_06,
    validate_ubl_dt_07,
    validate_ubl_sr_25,
    validate_ubl_sr_30,
    validate_ubl_sr_31,
    validate_ubl_cr_001,
    validate_ubl_cr_002,
    validate_ubl_cr_003,
    validate_ubl_cr_004,
    validate_ubl_cr_005,
    validate_ubl_cr_006,
    validate_ubl_cr_007,
    validate_ubl_cr_008,
    validate_ubl_cr_009,
    validate_ubl_cr_010,
    validate_ubl_cr_011,
    validate_ubl_cr_012,
    validate_ubl_cr_013,
    validate_ubl_cr_014,
    validate_ubl_cr_015,
    validate_ubl_cr_016,
    validate_ubl_cr_017,
    validate_ubl_cr_018,
    validate_ubl_cr_019,
    validate_ubl_cr_020,
    validate_ubl_cr_021,
    validate_ubl_cr_022,
    validate_ubl_cr_023,
    validate_ubl_cr_024,
    validate_ubl_cr_025,
    validate_ubl_cr_026,
    validate_ubl_cr_027,
    validate_ubl_cr_028,
    validate_ubl_cr_029,
    validate_ubl_cr_030,
    validate_ubl_cr_031,
    validate_ubl_cr_032,
    validate_ubl_cr_033,
    validate_ubl_cr_034,
    validate_ubl_cr_035,
    validate_ubl_cr_036,
    validate_ubl_cr_037,
    validate_ubl_cr_038,
    validate_ubl_cr_039,
    validate_ubl_cr_040,
    validate_ubl_cr_041,
    validate_ubl_cr_042,
    validate_ubl_cr_043,
    validate_ubl_cr_044,
    validate_ubl_cr_045,
    validate_ubl_cr_046,
    validate_ubl_cr_047,
    validate_ubl_cr_048,
    validate_ubl_cr_049,
    validate_ubl_cr_050,
    validate_ubl_cr_051,
    validate_ubl_cr_052,
    validate_ubl_cr_053,
    validate_ubl_cr_054,
    validate_ubl_cr_055,
    validate_ubl_cr_056,
    validate_ubl_cr_057,
    validate_ubl_cr_058,
    validate_ubl_cr_059,
    validate_ubl_cr_060,
    validate_ubl_cr_061,
    validate_ubl_cr_062,
    validate_ubl_cr_063,
    validate_ubl_cr_064,
    validate_ubl_cr_065,
    validate_ubl_cr_066,
    validate_ubl_cr_067,
    validate_ubl_cr_068,
    validate_ubl_cr_069,
    validate_ubl_cr_070,
    validate_ubl_cr_071,
    validate_ubl_cr_072,
    validate_ubl_cr_073,
    validate_ubl_cr_074,
    validate_ubl_cr_075,
    validate_ubl_cr_076,
    validate_ubl_cr_077,
    validate_ubl_cr_078,
    validate_ubl_cr_079,
    validate_ubl_cr_080,
    validate_ubl_cr_081,
    validate_ubl_cr_082,
    validate_ubl_cr_083,
    validate_ubl_cr_084,
    validate_ubl_cr_085,
    validate_ubl_cr_086,
    validate_ubl_cr_087,
    validate_ubl_cr_088,
    validate_ubl_cr_089,
    validate_ubl_cr_090,
    validate_ubl_cr_091,
    validate_ubl_cr_092,
    validate_ubl_cr_093,
    validate_ubl_cr_094,
    validate_ubl_cr_095,
    validate_ubl_cr_096,
    validate_ubl_cr_097,
    validate_ubl_cr_098,
    validate_ubl_cr_099,
    validate_ubl_cr_100,
    validate_ubl_cr_101,
    validate_ubl_cr_102,
    validate_ubl_cr_103,
    validate_ubl_cr_104,
    validate_ubl_cr_105,
    validate_ubl_cr_106,
    validate_ubl_cr_107,
    validate_ubl_cr_108,
    validate_ubl_cr_109,
    validate_ubl_cr_110,
    validate_ubl_cr_111,
    validate_ubl_cr_112,
    validate_ubl_cr_113,
    validate_ubl_cr_114,
    validate_ubl_cr_115,
    validate_ubl_cr_116,
    validate_ubl_cr_117,
    validate_ubl_cr_118,
    validate_ubl_cr_119,
    validate_ubl_cr_121,
    validate_ubl_cr_122,
    validate_ubl_cr_123,
    validate_ubl_cr_124,
    validate_ubl_cr_125,
    validate_ubl_cr_126,
    validate_ubl_cr_127,
    validate_ubl_cr_128,
    validate_ubl_cr_129,
    validate_ubl_cr_130,
    validate_ubl_cr_131,
    validate_ubl_cr_132,
    validate_ubl_cr_133,
    validate_ubl_cr_134,
    validate_ubl_cr_135,
    validate_ubl_cr_136,
    validate_ubl_cr_137,
    validate_ubl_cr_138,
    validate_ubl_cr_139,
    validate_ubl_cr_140,
    validate_ubl_cr_141,
    validate_ubl_cr_142,
    validate_ubl_cr_143,
    validate_ubl_cr_144,
    validate_ubl_cr_145,
    validate_ubl_cr_146,
    validate_ubl_cr_147,
    validate_ubl_cr_148,
    validate_ubl_cr_149,
    validate_ubl_cr_150,
    validate_ubl_cr_151,
    validate_ubl_cr_152,
    validate_ubl_cr_153,
    validate_ubl_cr_154,
    validate_ubl_cr_155,
    validate_ubl_cr_156,
    validate_ubl_cr_157,
    validate_ubl_cr_158,
    validate_ubl_cr_159,
    validate_ubl_cr_160,
    validate_ubl_cr_161,
    validate_ubl_cr_162,
    validate_ubl_cr_163,
    validate_ubl_cr_164,
    validate_ubl_cr_165,
    validate_ubl_cr_166,
    validate_ubl_cr_167,
    validate_ubl_cr_168,
    validate_ubl_cr_169,
    validate_ubl_cr_170,
    validate_ubl_cr_171,
    validate_ubl_cr_172,
    validate_ubl_cr_173,
    validate_ubl_cr_174,
    validate_ubl_cr_175,
    validate_ubl_cr_176,
    validate_ubl_cr_177,
    validate_ubl_cr_178,
    validate_ubl_cr_179,
    validate_ubl_cr_180,
    validate_ubl_cr_181,
    validate_ubl_cr_182,
    validate_ubl_cr_183,
    validate_ubl_cr_184,
    validate_ubl_cr_185,
    validate_ubl_cr_186,
    validate_ubl_cr_187,
    validate_ubl_cr_188,
    validate_ubl_cr_189,
    validate_ubl_cr_190,
    validate_ubl_cr_191,
    validate_ubl_cr_192,
    validate_ubl_cr_193,
    validate_ubl_cr_194,
    validate_ubl_cr_195,
    validate_ubl_cr_196,
    validate_ubl_cr_197,
    validate_ubl_cr_198,
    validate_ubl_cr_199,
    validate_ubl_cr_200,
    validate_ubl_cr_201,
    validate_ubl_cr_202,
    validate_ubl_cr_203,
    validate_ubl_cr_204,
    validate_ubl_cr_205,
    validate_ubl_cr_206,
    validate_ubl_cr_207,
    validate_ubl_cr_208,
    validate_ubl_cr_209,
    validate_ubl_cr_210,
    validate_ubl_cr_211,
    validate_ubl_cr_212,
    validate_ubl_cr_213,
    validate_ubl_cr_214,
    validate_ubl_cr_215,
    validate_ubl_cr_216,
    validate_ubl_cr_217,
    validate_ubl_cr_218,
    validate_ubl_cr_219,
    validate_ubl_cr_220,
    validate_ubl_cr_221,
    validate_ubl_cr_222,
    validate_ubl_cr_223,
    validate_ubl_cr_224,
    validate_ubl_cr_225,
    validate_ubl_cr_226,
    validate_ubl_cr_227,
    validate_ubl_cr_228,
    validate_ubl_cr_229,
    validate_ubl_cr_230,
    validate_ubl_cr_231,
    validate_ubl_cr_232,
    validate_ubl_cr_233,
    validate_ubl_cr_234,
    validate_ubl_cr_235,
    validate_ubl_cr_236,
    validate_ubl_cr_237,
    validate_ubl_cr_238,
    validate_ubl_cr_239,
    validate_ubl_cr_240,
    validate_ubl_cr_241,
    validate_ubl_cr_242,
    validate_ubl_cr_243,
    validate_ubl_cr_244,
    validate_ubl_cr_245,
    validate_ubl_cr_246,
    validate_ubl_cr_247,
    validate_ubl_cr_248,
    validate_ubl_cr_249,
    validate_ubl_cr_250,
    validate_ubl_cr_251,
    validate_ubl_cr_252,
    validate_ubl_cr_253,
    validate_ubl_cr_254,
    validate_ubl_cr_255,
    validate_ubl_cr_256,
    validate_ubl_cr_257,
    validate_ubl_cr_258,
    validate_ubl_cr_259,
    validate_ubl_cr_260,
    validate_ubl_cr_261,
    validate_ubl_cr_262,
    validate_ubl_cr_263,
    validate_ubl_cr_264,
    validate_ubl_cr_265,
    validate_ubl_cr_266,
    validate_ubl_cr_267,
    validate_ubl_cr_268,
    validate_ubl_cr_269,
    validate_ubl_cr_270,
    validate_ubl_cr_271,
    validate_ubl_cr_272,
    validate_ubl_cr_273,
    validate_ubl_cr_274,
    validate_ubl_cr_275,
    validate_ubl_cr_276,
    validate_ubl_cr_277,
    validate_ubl_cr_278,
    validate_ubl_cr_279,
    validate_ubl_cr_280,
    validate_ubl_cr_281,
    validate_ubl_cr_282,
    validate_ubl_cr_283,
    validate_ubl_cr_284,
    validate_ubl_cr_285,
    validate_ubl_cr_286,
    validate_ubl_cr_287,
    validate_ubl_cr_288,
    validate_ubl_cr_289,
    validate_ubl_cr_290,
    validate_ubl_cr_291,
    validate_ubl_cr_292,
    validate_ubl_cr_293,
    validate_ubl_cr_294,
    validate_ubl_cr_295,
    validate_ubl_cr_296,
    validate_ubl_cr_297,
    validate_ubl_cr_298,
    validate_ubl_cr_299,
    validate_ubl_cr_300,
    validate_ubl_cr_301,
    validate_ubl_cr_302,
    validate_ubl_cr_303,
    validate_ubl_cr_304,
    validate_ubl_cr_305,
    validate_ubl_cr_306,
    validate_ubl_cr_307,
    validate_ubl_cr_308,
    validate_ubl_cr_309,
    validate_ubl_cr_310,
    validate_ubl_cr_311,
    validate_ubl_cr_312,
    validate_ubl_cr_313,
    validate_ubl_cr_314,
    validate_ubl_cr_315,
    validate_ubl_cr_316,
    validate_ubl_cr_317,
    validate_ubl_cr_318,
    validate_ubl_cr_319,
    validate_ubl_cr_320,
    validate_ubl_cr_321,
    validate_ubl_cr_322,
    validate_ubl_cr_323,
    validate_ubl_cr_324,
    validate_ubl_cr_325,
    validate_ubl_cr_326,
    validate_ubl_cr_327,
    validate_ubl_cr_328,
    validate_ubl_cr_329,
    validate_ubl_cr_330,
    validate_ubl_cr_331,
    validate_ubl_cr_332,
    validate_ubl_cr_333,
    validate_ubl_cr_334,
    validate_ubl_cr_335,
    validate_ubl_cr_336,
    validate_ubl_cr_337,
    validate_ubl_cr_338,
    validate_ubl_cr_339,
    validate_ubl_cr_340,
    validate_ubl_cr_341,
    validate_ubl_cr_342,
    validate_ubl_cr_343,
    validate_ubl_cr_344,
    validate_ubl_cr_345,
    validate_ubl_cr_346,
    validate_ubl_cr_347,
    validate_ubl_cr_348,
    validate_ubl_cr_349,
    validate_ubl_cr_350,
    validate_ubl_cr_351,
    validate_ubl_cr_352,
    validate_ubl_cr_353,
    validate_ubl_cr_354,
    validate_ubl_cr_355,
    validate_ubl_cr_356,
    validate_ubl_cr_357,
    validate_ubl_cr_358,
    validate_ubl_cr_359,
    validate_ubl_cr_360,
    validate_ubl_cr_361,
    validate_ubl_cr_362,
    validate_ubl_cr_363,
    validate_ubl_cr_364,
    validate_ubl_cr_365,
    validate_ubl_cr_366,
    validate_ubl_cr_367,
    validate_ubl_cr_368,
    validate_ubl_cr_369,
    validate_ubl_cr_370,
    validate_ubl_cr_371,
    validate_ubl_cr_372,
    validate_ubl_cr_373,
    validate_ubl_cr_374,
    validate_ubl_cr_375,
    validate_ubl_cr_376,
    validate_ubl_cr_377,
    validate_ubl_cr_378,
    validate_ubl_cr_379,
    validate_ubl_cr_380,
    validate_ubl_cr_381,
    validate_ubl_cr_382,
    validate_ubl_cr_383,
    validate_ubl_cr_384,
    validate_ubl_cr_385,
    validate_ubl_cr_386,
    validate_ubl_cr_387,
    validate_ubl_cr_388,
    validate_ubl_cr_389,
    validate_ubl_cr_390,
    validate_ubl_cr_391,
    validate_ubl_cr_392,
    validate_ubl_cr_393,
    validate_ubl_cr_394,
    validate_ubl_cr_395,
    validate_ubl_cr_396,
    validate_ubl_cr_397,
    validate_ubl_cr_398,
    validate_ubl_cr_399,
    validate_ubl_cr_400,
    validate_ubl_cr_401,
    validate_ubl_cr_402,
    validate_ubl_cr_403,
    validate_ubl_cr_404,
    validate_ubl_cr_405,
    validate_ubl_cr_406,
    validate_ubl_cr_407,
    validate_ubl_cr_408,
    validate_ubl_cr_409,
    validate_ubl_cr_410,
    validate_ubl_cr_411,
    validate_ubl_cr_412,
    validate_ubl_cr_413,
    validate_ubl_cr_414,
    validate_ubl_cr_415,
    validate_ubl_cr_416,
    validate_ubl_cr_417,
    validate_ubl_cr_418,
    validate_ubl_cr_419,
    validate_ubl_cr_420,
    validate_ubl_cr_421,
    validate_ubl_cr_422,
    validate_ubl_cr_424,
    validate_ubl_cr_425,
    validate_ubl_cr_426,
    validate_ubl_cr_427,
    validate_ubl_cr_428,
    validate_ubl_cr_429,
    validate_ubl_cr_430,
    validate_ubl_cr_431,
    validate_ubl_cr_432,
    validate_ubl_cr_433,
    validate_ubl_cr_434,
    validate_ubl_cr_435,
    validate_ubl_cr_436,
    validate_ubl_cr_437,
    validate_ubl_cr_438,
    validate_ubl_cr_439,
    validate_ubl_cr_440,
    validate_ubl_cr_441,
    validate_ubl_cr_442,
    validate_ubl_cr_443,
    validate_ubl_cr_444,
    validate_ubl_cr_445,
    validate_ubl_cr_446,
    validate_ubl_cr_447,
    validate_ubl_cr_448,
    validate_ubl_cr_449,
    validate_ubl_cr_450,
    validate_ubl_cr_451,
    validate_ubl_cr_452,
    validate_ubl_cr_453,
    validate_ubl_cr_454,
    validate_ubl_cr_455,
    validate_ubl_cr_456,
    validate_ubl_cr_457,
    validate_ubl_cr_458,
    validate_ubl_cr_459,
    validate_ubl_cr_460,
    validate_ubl_cr_461,
    validate_ubl_cr_462,
    validate_ubl_cr_463,
    validate_ubl_cr_464,
    validate_ubl_cr_465,
    validate_ubl_cr_466,
    validate_ubl_cr_467,
    validate_ubl_cr_468,
    validate_ubl_cr_469,
    validate_ubl_cr_470,
    validate_ubl_cr_471,
    validate_ubl_cr_472,
    validate_ubl_cr_473,
    validate_ubl_cr_474,
    validate_ubl_cr_475,
    validate_ubl_cr_476,
    validate_ubl_cr_477,
    validate_ubl_cr_478,
    validate_ubl_cr_479,
    validate_ubl_cr_480,
    validate_ubl_cr_481,
    validate_ubl_cr_482,
    validate_ubl_cr_483,
    validate_ubl_cr_484,
    validate_ubl_cr_485,
    validate_ubl_cr_486,
    validate_ubl_cr_487,
    validate_ubl_cr_488,
    validate_ubl_cr_489,
    validate_ubl_cr_490,
    validate_ubl_cr_491,
    validate_ubl_cr_492,
    validate_ubl_cr_493,
    validate_ubl_cr_494,
    validate_ubl_cr_495,
    validate_ubl_cr_496,
    validate_ubl_cr_497,
    validate_ubl_cr_498,
    validate_ubl_cr_499,
    validate_ubl_cr_500,
    validate_ubl_cr_501,
    validate_ubl_cr_502,
    validate_ubl_cr_503,
    validate_ubl_cr_504,
    validate_ubl_cr_505,
    validate_ubl_cr_506,
    validate_ubl_cr_507,
    validate_ubl_cr_508,
    validate_ubl_cr_509,
    validate_ubl_cr_510,
    validate_ubl_cr_511,
    validate_ubl_cr_512,
    validate_ubl_cr_513,
    validate_ubl_cr_514,
    validate_ubl_cr_515,
    validate_ubl_cr_516,
    validate_ubl_cr_517,
    validate_ubl_cr_518,
    validate_ubl_cr_519,
    validate_ubl_cr_520,
    validate_ubl_cr_521,
    validate_ubl_cr_522,
    validate_ubl_cr_523,
    validate_ubl_cr_524,
    validate_ubl_cr_525,
    validate_ubl_cr_526,
    validate_ubl_cr_527,
    validate_ubl_cr_528,
    validate_ubl_cr_529,
    validate_ubl_cr_530,
    validate_ubl_cr_531,
    validate_ubl_cr_532,
    validate_ubl_cr_533,
    validate_ubl_cr_534,
    validate_ubl_cr_535,
    validate_ubl_cr_537,
    validate_ubl_cr_538,
    validate_ubl_cr_539,
    validate_ubl_cr_540,
    validate_ubl_cr_541,
    validate_ubl_cr_542,
    validate_ubl_cr_543,
    validate_ubl_cr_544,
    validate_ubl_cr_545,
    validate_ubl_cr_546,
    validate_ubl_cr_547,
    validate_ubl_cr_548,
    validate_ubl_cr_549,
    validate_ubl_cr_550,
    validate_ubl_cr_551,
    validate_ubl_cr_552,
    validate_ubl_cr_553,
    validate_ubl_cr_554,
    validate_ubl_cr_555,
    validate_ubl_cr_556,
    validate_ubl_cr_557,
    validate_ubl_cr_558,
    validate_ubl_cr_559,
    validate_ubl_cr_560,
    validate_ubl_cr_561,
    validate_ubl_cr_562,
    validate_ubl_cr_563,
    validate_ubl_cr_564,
    validate_ubl_cr_565,
    validate_ubl_cr_566,
    validate_ubl_cr_567,
    validate_ubl_cr_568,
    validate_ubl_cr_569,
    validate_ubl_cr_570,
    validate_ubl_cr_571,
    validate_ubl_cr_572,
    validate_ubl_cr_573,
    validate_ubl_cr_574,
    validate_ubl_cr_575,
    validate_ubl_cr_576,
    validate_ubl_cr_577,
    validate_ubl_cr_578,
    validate_ubl_cr_579,
    validate_ubl_cr_580,
    validate_ubl_cr_581,
    validate_ubl_cr_582,
    validate_ubl_cr_583,
    validate_ubl_cr_584,
    validate_ubl_cr_585,
    validate_ubl_cr_586,
    validate_ubl_cr_587,
    validate_ubl_cr_588,
    validate_ubl_cr_589,
    validate_ubl_cr_590,
    validate_ubl_cr_591,
    validate_ubl_cr_592,
    validate_ubl_cr_593,
    validate_ubl_cr_594,
    validate_ubl_cr_595,
    validate_ubl_cr_596,
    validate_ubl_cr_597,
    validate_ubl_cr_598,
    validate_ubl_cr_599,
    validate_ubl_cr_600,
    validate_ubl_cr_601,
    validate_ubl_cr_602,
    validate_ubl_cr_603,
    validate_ubl_cr_604,
    validate_ubl_cr_605,
    validate_ubl_cr_606,
    validate_ubl_cr_607,
    validate_ubl_cr_608,
    validate_ubl_cr_609,
    validate_ubl_cr_610,
    validate_ubl_cr_611,
    validate_ubl_cr_612,
    validate_ubl_cr_613,
    validate_ubl_cr_614,
    validate_ubl_cr_615,
    validate_ubl_cr_616,
    validate_ubl_cr_617,
    validate_ubl_cr_618,
    validate_ubl_cr_619,
    validate_ubl_cr_620,
    validate_ubl_cr_621,
    validate_ubl_cr_622,
    validate_ubl_cr_623,
    validate_ubl_cr_624,
    validate_ubl_cr_625,
    validate_ubl_cr_626,
    validate_ubl_cr_627,
    validate_ubl_cr_628,
    validate_ubl_cr_629,
    validate_ubl_cr_630,
    validate_ubl_cr_631,
    validate_ubl_cr_632,
    validate_ubl_cr_633,
    validate_ubl_cr_634,
    validate_ubl_cr_635,
    validate_ubl_cr_636,
    validate_ubl_cr_637,
    validate_ubl_cr_638,
    validate_ubl_cr_639,
    validate_ubl_cr_640,
    validate_ubl_cr_641,
    validate_ubl_cr_642,
    validate_ubl_cr_643,
    validate_ubl_cr_644,
    validate_ubl_cr_645,
    validate_ubl_cr_646,
    validate_ubl_cr_647,
    validate_ubl_cr_648,
    validate_ubl_cr_649,
    validate_ubl_cr_650,
    validate_ubl_cr_651,
    validate_ubl_cr_652,
    validate_ubl_cr_653,
    validate_ubl_cr_654,
    validate_ubl_cr_655,
    validate_ubl_cr_656,
    validate_ubl_cr_657,
    validate_ubl_cr_658,
    validate_ubl_cr_659,
    validate_ubl_cr_660,
    validate_ubl_cr_661,
    validate_ubl_cr_662,
    validate_ubl_cr_663,
    validate_ubl_cr_664,
    validate_ubl_cr_665,
    validate_ubl_cr_666,
    validate_ubl_cr_667,
    validate_ubl_cr_668,
    validate_ubl_cr_669,
    validate_ubl_cr_670,
    validate_ubl_cr_671,
    validate_ubl_cr_672,
    validate_ubl_cr_673,
    validate_ubl_cr_674,
    validate_ubl_cr_675,
    validate_ubl_cr_676,
    validate_ubl_cr_677,
    validate_ubl_cr_678,
    validate_ubl_cr_679,
    validate_ubl_cr_680,
    validate_ubl_cr_681,
    validate_ubl_cr_682,
    validate_ubl_dt_08,
    validate_ubl_dt_09,
    validate_ubl_dt_10,
    validate_ubl_dt_11,
    validate_ubl_dt_12,
    validate_ubl_dt_13,
    validate_ubl_dt_14,
    validate_ubl_dt_15,
    validate_ubl_dt_16,
    validate_ubl_dt_17,
    validate_ubl_dt_18,
    validate_ubl_dt_19,
    validate_ubl_dt_20,
    validate_ubl_dt_21,
    validate_ubl_dt_22,
    validate_ubl_dt_23,
    validate_ubl_dt_24,
    validate_ubl_dt_25,
    validate_ubl_dt_26,
    validate_ubl_dt_27,
    validate_ubl_dt_28,
    validate_ubl_sr_01,
    validate_ubl_sr_02,
    validate_ubl_sr_03,
    validate_ubl_sr_04,
    validate_ubl_sr_05,
    validate_ubl_sr_08,
    validate_ubl_sr_09,
    validate_ubl_sr_10,
    validate_ubl_sr_11,
    validate_ubl_sr_12,
    validate_ubl_sr_13,
    validate_ubl_sr_14,
    validate_ubl_sr_15,
    validate_ubl_sr_16,
    validate_ubl_sr_17,
    validate_ubl_sr_18,
    validate_ubl_sr_24,
    validate_ubl_sr_29,
    validate_ubl_sr_39,
    validate_ubl_sr_40,
    validate_ubl_sr_44,
    validate_ubl_sr_45,
    validate_ubl_sr_46,
    validate_ubl_sr_47,
    validate_ubl_sr_49,
    validate_ubl_sr_34,
    validate_ubl_sr_35,
    validate_ubl_sr_36,
    validate_ubl_sr_37,
    validate_ubl_sr_48,
    validate_ubl_sr_50,
    validate_ubl_sr_52,
    validate_ubl_sr_19,
    validate_ubl_sr_20,
    validate_ubl_sr_21,
    validate_ubl_sr_26,
    validate_ubl_sr_27,
    validate_ubl_sr_28,
    validate_ubl_sr_06,
    validate_ubl_sr_07,
    validate_ubl_sr_22,
    validate_ubl_sr_23,
    validate_ubl_sr_32,
    validate_br_cl_01,
    validate_br_cl_03,
    validate_br_cl_04,
    validate_br_cl_05,
    validate_br_cl_06,
    validate_br_cl_07,
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

// Context: cac:AdditionalDocumentReference
// Test: normalize-space(cbc:ID) != ''
fn validate_br_52(invoice: &UblInvoice) -> Result<(), ValidationError> {
    if invoice
        .cac_additional_document_reference
        .iter()
        .any(|doc| doc.id.is_none())
    {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-52",
            "[BR-52]-Each Additional supporting document (BG-24) shall contain a Supporting document reference (BT-122).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:LegalMonetaryTotal/cbc:PayableAmount
// Test: ((. > 0) and (exists(//cbc:DueDate) or exists(//cac:PaymentTerms/cbc:Note))) or (. <= 0)
fn validate_br_co_25(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-25",
            "[BR-CO-25]-In case the Amount due for payment (BT-115) is positive, either the Payment due date (BT-9) or the Payment terms (BT-20) shall be present.",
        )));
    }
    Ok(())
}

// Context: cac:AccountingCustomerParty/cac:Party/cbc:EndpointID
// Test: exists(@schemeID)
fn validate_br_63(invoice: &UblInvoice) -> Result<(), ValidationError> {
    if let Some(party) = &invoice.cac_accounting_customer_party {
        if let Some(cac_party) = &party.cac_party {
            if let Some(cbc_endpoint_id) = &cac_party.cbc_endpoint_id {
                if cbc_endpoint_id.scheme_id.is_none() {
                    return Err(ValidationError::Fatal(BusinessRuleViolation::new(
                        "BR-63",
                        "[BR-63]-The Buyer electronic address (BT-49) shall have a Scheme identifier.",
                    )));
                }
            }
        }
    }

    Ok(())
}

// Context: cac:AccountingCustomerParty/cac:Party/cac:PostalAddress
// Test: normalize-space(cac:Country/cbc:IdentificationCode) != ''
fn validate_br_11(invoice: &UblInvoice) -> Result<(), ValidationError> {
    if let Some(party) = &invoice.cac_accounting_customer_party {
        if let Some(cac_party) = &party.cac_party {
            if let Some(cac_postal_address) = &cac_party.cac_postal_address {
                if cac_postal_address.cac_country.is_none() {
                    return Err(ValidationError::Fatal(BusinessRuleViolation::new(
                        "BR-11",
                        "[BR-11]-The Buyer postal address shall contain a Buyer country code (BT-55).",
                    )));
                }
            }
        }
    }
    Ok(())
}

// Context: cac:PaymentMeans/cac:CardAccount/cbc:PrimaryAccountNumberID
// Test: string-length(normalize-space(.))<=10
fn validate_br_51(invoice: &UblInvoice) -> Result<(), ValidationError> {
    if invoice.cac_payment_means.iter().any(|payment_means| {
        payment_means.cac_card_account.iter().any(|card_account| {
            card_account
                .primary_account_number_id
                .iter()
                .any(|identifier| identifier.id.iter().any(|id| id.len() > 10))
        })
    }) {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "BR-51",
            "[BR-51]-In accordance with card payments security standards an invoice should never include a full card primary account number (BT-87). At the moment PCI Security Standards Council has defined that the first 6 digits and last 4 digits are the maximum number of digits to be shown.",
        )));
    }
    Ok(())
}

// Context: cac:Delivery/cac:DeliveryLocation/cac:Address
// Test: exists(cac:Country/cbc:IdentificationCode)
fn validate_br_57(invoice: &UblInvoice) -> Result<(), ValidationError> {
    if invoice.cac_delivery.iter().any(|delivery| {
        delivery
            .cac_delivery_location
            .iter()
            .any(|delivery_location| {
                delivery_location
                    .cac_address
                    .iter()
                    .any(|address| address.cac_country.is_none())
            })
    }) {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-57",
            "[BR-57]-Each Deliver to address (BG-15) shall contain a Deliver to country code (BT-80).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = false()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: exists(cbc:Amount)
fn validate_br_31(invoice: &UblInvoice) -> Result<(), ValidationError> {
    if invoice.cac_allowance_charge.iter().any(|allowance_charge| {
        allowance_charge
            .charge_indicator
            .map(|charge| !charge)
            .unwrap_or(false)
            && allowance_charge.amount.is_none()
    }) {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-31",
            "[BR-31]-Each Document level allowance (BG-20) shall have a Document level allowance amount (BT-92).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = false()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: exists(cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID)
fn validate_br_32(invoice: &UblInvoice) -> Result<(), ValidationError> {
    if invoice.cac_allowance_charge.iter().any(|allowance_charge| {
        allowance_charge
            .charge_indicator
            .map(|charge| !charge)
            .unwrap_or(false)
            && allowance_charge
                .cac_tax_category
                .iter()
                .all(|tax_category| {
                    tax_category.cac_tax_scheme.iter().any(|tax_scheme| {
                        tax_scheme
                            .id
                            .iter()
                            .any(|identifier| identifier.id.iter().any(|id| id != "VAT"))
                    })
                })
    }) {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-32",
            "[BR-32]-Each Document level allowance (BG-20) shall have a Document level allowance VAT category code (BT-95).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = false()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: exists(cbc:AllowanceChargeReason) or exists(cbc:AllowanceChargeReasonCode)
fn validate_br_33(invoice: &UblInvoice) -> Result<(), ValidationError> {
    if invoice.cac_allowance_charge.iter().any(|allowance_charge| {
        allowance_charge
            .charge_indicator
            .map(|charge_indicator| !charge_indicator)
            .unwrap_or(false)
            && allowance_charge.cbc_allowance_charge_reason.is_none()
            && allowance_charge.cbc_allowance_charge_reason_code.is_none()
    }) {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-33",
            "[BR-33]-Each Document level allowance (BG-20) shall have a Document level allowance reason (BT-97) or a Document level allowance reason code (BT-98).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = false()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: true()
fn validate_br_co_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-05",
            "[BR-CO-05]-Document level allowance reason code (BT-98) and Document level allowance reason (BT-97) shall indicate the same type of allowance.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = false()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: exists(cbc:AllowanceChargeReason) or exists(cbc:AllowanceChargeReasonCode)
fn validate_br_co_21(invoice: &UblInvoice) -> Result<(), ValidationError> {
    if invoice.cac_allowance_charge.iter().any(|allowance_charge| {
        allowance_charge
            .charge_indicator
            .map(|charge_indicator| !charge_indicator)
            .unwrap_or(false)
            && allowance_charge.cbc_allowance_charge_reason.is_none()
            && allowance_charge.cbc_allowance_charge_reason_code.is_none()
    }) {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-21",
            "[BR-CO-21]-Each Document level allowance (BG-20) shall contain a Document level allowance reason (BT-97) or a Document level allowance reason code (BT-98), or both.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = false()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: string-length(substring-after(cbc:Amount,'.'))<=2
fn validate_br_dec_01(invoice: &UblInvoice) -> Result<(), ValidationError> {
    if invoice.cac_allowance_charge.iter().any(|allowance_charge| {
        allowance_charge
            .charge_indicator
            .iter()
            .any(|charge_indicator| {
                !charge_indicator
                    && allowance_charge
                        .amount
                        .iter()
                        .any(|amount| amount.value.iter().any(|value| value.scale() > 2))
            })
    }) {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-01",
            "[BR-DEC-01]-The allowed maximum number of decimals for the Document level allowance amount (BT-92) is 2.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = false()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: string-length(substring-after(cbc:BaseAmount,'.'))<=2
fn validate_br_dec_02(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-02",
            "[BR-DEC-02]-The allowed maximum number of decimals for the Document level allowance base amount (BT-93) is 2.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = true()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: exists(cbc:Amount)
fn validate_br_36(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-36",
            "[BR-36]-Each Document level charge (BG-21) shall have a Document level charge amount (BT-99).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = true()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: exists(cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID)
fn validate_br_37(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-37",
            "[BR-37]-Each Document level charge (BG-21) shall have a Document level charge VAT category code (BT-102).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = true()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: exists(cbc:AllowanceChargeReason) or exists(cbc:AllowanceChargeReasonCode)
fn validate_br_38(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-38",
            "[BR-38]-Each Document level charge (BG-21) shall have a Document level charge reason (BT-104) or a Document level charge reason code (BT-105).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = true()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: true()
fn validate_br_co_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-06",
            "[BR-CO-06]-Document level charge reason code (BT-105) and Document level charge reason (BT-104) shall indicate the same type of charge.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = true()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: exists(cbc:AllowanceChargeReason) or exists(cbc:AllowanceChargeReasonCode)
fn validate_br_co_22(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-22",
            "[BR-CO-22]-Each Document level charge (BG-21) shall contain a Document level charge reason (BT-104) or a Document level charge reason code (BT-105), or both.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = true()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: string-length(substring-after(cbc:Amount,'.'))<=2
fn validate_br_dec_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-05",
            "[BR-DEC-05]-The allowed maximum number of decimals for the Document level charge amount (BT-99) is 2.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:AllowanceCharge[cbc:ChargeIndicator = true()] | /cn:CreditNote/cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: string-length(substring-after(cbc:BaseAmount,'.'))<=2
fn validate_br_dec_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-06",
            "[BR-DEC-06]-The allowed maximum number of decimals for the Document level charge base amount (BT-100) is 2.",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: exists(cbc:LineExtensionAmount)
fn validate_br_12(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-12",
            "[BR-12]-An Invoice shall have the Sum of Invoice line net amount (BT-106).",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: exists(cbc:TaxExclusiveAmount)
fn validate_br_13(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-13",
            "[BR-13]-An Invoice shall have the Invoice total amount without VAT (BT-109).",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: exists(cbc:TaxInclusiveAmount)
fn validate_br_14(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-14",
            "[BR-14]-An Invoice shall have the Invoice total amount with VAT (BT-112).",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: exists(cbc:PayableAmount)
fn validate_br_15(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-15",
            "[BR-15]-An Invoice shall have the Amount due for payment (BT-115).",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: (xs:decimal(cbc:LineExtensionAmount) = xs:decimal(round(sum(//(cac:InvoiceLine|cac:CreditNoteLine)/xs:decimal(cbc:LineExtensionAmount)) * 10 * 10) div 100))
fn validate_br_co_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-10",
            "[BR-CO-10]-Sum of Invoice line net amount (BT-106) = Σ Invoice line net amount (BT-131).",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: xs:decimal(cbc:AllowanceTotalAmount) = (round(sum(../cac:AllowanceCharge[cbc:ChargeIndicator=false()]/xs:decimal(cbc:Amount)) * 10 * 10) div 100) or  (not(cbc:AllowanceTotalAmount) and not(../cac:AllowanceCharge[cbc:ChargeIndicator=false()]))
fn validate_br_co_11(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-11",
            "[BR-CO-11]-Sum of allowances on document level (BT-107) = Σ Document level allowance amount (BT-92).",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: xs:decimal(cbc:ChargeTotalAmount) = (round(sum(../cac:AllowanceCharge[cbc:ChargeIndicator=true()]/xs:decimal(cbc:Amount)) * 10 * 10) div 100) or (not(cbc:ChargeTotalAmount) and not(../cac:AllowanceCharge[cbc:ChargeIndicator=true()]))
fn validate_br_co_12(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-12",
            "[BR-CO-12]-Sum of charges on document level (BT-108) = Σ Document level charge amount (BT-99).",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: ((cbc:ChargeTotalAmount) and (cbc:AllowanceTotalAmount) and (xs:decimal(cbc:TaxExclusiveAmount) = round((xs:decimal(cbc:LineExtensionAmount) + xs:decimal(cbc:ChargeTotalAmount) - xs:decimal(cbc:AllowanceTotalAmount)) * 10 * 10) div 100 ))  or (not(cbc:ChargeTotalAmount) and (cbc:AllowanceTotalAmount) and (xs:decimal(cbc:TaxExclusiveAmount) = round((xs:decimal(cbc:LineExtensionAmount) - xs:decimal(cbc:AllowanceTotalAmount)) * 10 * 10 ) div 100)) or ((cbc:ChargeTotalAmount) and not(cbc:AllowanceTotalAmount) and (xs:decimal(cbc:TaxExclusiveAmount) = round((xs:decimal(cbc:LineExtensionAmount) + xs:decimal(cbc:ChargeTotalAmount)) * 10 * 10 ) div 100)) or (not(cbc:ChargeTotalAmount) and not(cbc:AllowanceTotalAmount) and (xs:decimal(cbc:TaxExclusiveAmount) = xs:decimal(cbc:LineExtensionAmount)))
fn validate_br_co_13(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-13",
            "[BR-CO-13]-Invoice total amount without VAT (BT-109) = Σ Invoice line net amount (BT-131) - Sum of allowances on document level (BT-107) + Sum of charges on document level (BT-108).",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: (exists(cbc:PrepaidAmount) and not(exists(cbc:PayableRoundingAmount)) and (xs:decimal(cbc:PayableAmount) = (round((xs:decimal(cbc:TaxInclusiveAmount) - xs:decimal(cbc:PrepaidAmount)) * 10 * 10) div 100))) or (not(exists(cbc:PrepaidAmount)) and not(exists(cbc:PayableRoundingAmount)) and xs:decimal(cbc:PayableAmount) = xs:decimal(cbc:TaxInclusiveAmount)) or (exists(cbc:PrepaidAmount) and exists(cbc:PayableRoundingAmount) and ((round((xs:decimal(cbc:PayableAmount) - xs:decimal(cbc:PayableRoundingAmount)) * 10 * 10) div 100) = (round((xs:decimal(cbc:TaxInclusiveAmount) - xs:decimal(cbc:PrepaidAmount)) * 10 * 10) div 100))) or  (not(exists(cbc:PrepaidAmount)) and exists(cbc:PayableRoundingAmount) and ((round((xs:decimal(cbc:PayableAmount) - xs:decimal(cbc:PayableRoundingAmount)) * 10 * 10) div 100) = xs:decimal(cbc:TaxInclusiveAmount)))
fn validate_br_co_16(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-16",
            "[BR-CO-16]-Amount due for payment (BT-115) = Invoice total amount with VAT (BT-112) -Paid amount (BT-113) +Rounding amount (BT-114).",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: string-length(substring-after(cbc:LineExtensionAmount,'.'))<=2
fn validate_br_dec_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-09",
            "[BR-DEC-09]-The allowed maximum number of decimals for the Sum of Invoice line net amount (BT-106) is 2.",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: string-length(substring-after(cbc:AllowanceTotalAmount,'.'))<=2
fn validate_br_dec_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-10",
            "[BR-DEC-10]-The allowed maximum number of decimals for the Sum of allowanced on document level (BT-107) is 2.",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: string-length(substring-after(cbc:ChargeTotalAmount,'.'))<=2
fn validate_br_dec_11(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-11",
            "[BR-DEC-11]-The allowed maximum number of decimals for the Sum of charges on document level (BT-108) is 2.",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: string-length(substring-after(cbc:TaxExclusiveAmount,'.'))<=2
fn validate_br_dec_12(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-12",
            "[BR-DEC-12]-The allowed maximum number of decimals for the Invoice total amount without VAT (BT-109) is 2.",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: string-length(substring-after(cbc:TaxInclusiveAmount,'.'))<=2
fn validate_br_dec_14(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-14",
            "[BR-DEC-14]-The allowed maximum number of decimals for the Invoice total amount with VAT (BT-112) is 2.",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: string-length(substring-after(cbc:PrepaidAmount,'.'))<=2
fn validate_br_dec_16(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-16",
            "[BR-DEC-16]-The allowed maximum number of decimals for the Paid amount (BT-113) is 2.",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: string-length(substring-after(cbc:PayableRoundingAmount,'.'))<=2
fn validate_br_dec_17(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-17",
            "[BR-DEC-17]-The allowed maximum number of decimals for the Rounding amount (BT-114) is 2.",
        )));
    }
    Ok(())
}

// Context: cac:LegalMonetaryTotal
// Test: string-length(substring-after(cbc:PayableAmount,'.'))<=2
fn validate_br_dec_18(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-18",
            "[BR-DEC-18]-The allowed maximum number of decimals for the Amount due for payment (BT-115) is 2.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: normalize-space(cbc:CustomizationID) != ''
fn validate_br_01(invoice: &UblInvoice) -> Result<(), ValidationError> {
    if !invoice
        .cbc_customization_id
        .iter()
        .any(|customization_id| !customization_id.is_empty())
    {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-01",
            "[BR-01]-An Invoice shall have a Specification identifier (BT-24).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: normalize-space(cbc:ID) != ''
fn validate_br_02(invoice: &UblInvoice) -> Result<(), ValidationError> {
    // cbc id is mandatory
    if !invoice.cbc_id.iter().any(|id| !id.is_empty()) {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-02",
            "[BR-02]-An Invoice shall have an Invoice number (BT-1).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: normalize-space(cbc:IssueDate) != ''
fn validate_br_03(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-03",
            "[BR-03]-An Invoice shall have an Invoice issue date (BT-2).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: normalize-space(cbc:InvoiceTypeCode) != '' or normalize-space(cbc:CreditNoteTypeCode) !=''
fn validate_br_04(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-04",
            "[BR-04]-An Invoice shall have an Invoice type code (BT-3).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: normalize-space(cbc:DocumentCurrencyCode) != ''
fn validate_br_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-05",
            "[BR-05]-An Invoice shall have an Invoice currency code (BT-5).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: normalize-space(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:RegistrationName) != ''
fn validate_br_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-06",
            "[BR-06]-An Invoice shall contain the Seller name (BT-27).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: normalize-space(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:RegistrationName) != ''
fn validate_br_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-07",
            "[BR-07]-An Invoice shall contain the Buyer name (BT-44).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: exists(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress)
fn validate_br_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-08",
            "[BR-08]-An Invoice shall contain the Seller postal address.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: exists(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress)
fn validate_br_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-10",
            "[BR-10]-An Invoice shall contain the Buyer postal address (BG-8).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: exists(cac:InvoiceLine) or exists(cac:CreditNoteLine)
fn validate_br_16(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-16",
            "[BR-16]-An Invoice shall have at least one Invoice line (BG-25)",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: every $taxcurrency in cbc:TaxCurrencyCode satisfies exists(//cac:TaxTotal/cbc:TaxAmount[@currencyID=$taxcurrency])
fn validate_br_53(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-53",
            "[BR-53]-If the VAT accounting currency code (BT-6) is present, then the Invoice total VAT amount in accounting currency (BT-111) shall be provided.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: count(cac:PaymentMeans/cac:CardAccount) <= 1
fn validate_br_66(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-66",
            "[BR-66]-An Invoice shall contain maximum one Payment Card account (BG-18).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: count(cac:PaymentMeans/cac:PaymentMandate) <= 1
fn validate_br_67(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-67",
            "[BR-67]-An Invoice shall contain maximum one Payment Mandate (BG-19).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: ((exists(//cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'AE']) or exists(//cac:ClassifiedTaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'AE'])) and (count(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'AE']) = 1)) or (not(//cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'AE']) and not(//cac:ClassifiedTaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'AE']))
fn validate_br_ae_01(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-01",
            "[BR-AE-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Reverse charge\" shall contain in the VAT Breakdown (BG-23) exactly one VAT category code (BT-118) equal with \"VAT reverse charge\".",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'AE'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID)) and (exists(//cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) or exists(//cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:CompanyID))) or not(exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'AE'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_ae_02(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-02",
            "[BR-AE-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Reverse charge\" shall contain the Seller VAT Identifier (BT-31), the Seller Tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48) and/or the Buyer legal registration identifier (BT-47).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID) = 'AE'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID)) and (exists(//cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) or exists(//cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID) = 'AE'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_ae_03(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-03",
            "[BR-AE-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Reverse charge\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48) and/or the Buyer legal registration identifier (BT-47).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID) = 'AE'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID)) and (exists(//cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) or exists(//cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID) = 'AE'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_ae_04(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-04",
            "[BR-AE-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Reverse charge\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48) and/or the Buyer legal registration identifier (BT-47).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(cbc:TaxPointDate) and not(cac:InvoicePeriod/cbc:DescriptionCode)) or (not(cbc:TaxPointDate) and exists(cac:InvoicePeriod/cbc:DescriptionCode)) or (not(cbc:TaxPointDate) and not(cac:InvoicePeriod/cbc:DescriptionCode))
fn validate_br_co_03(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-03",
            "[BR-CO-03]-Value added tax point date (BT-7) and Value added tax point date code (BT-8) are mutually exclusive.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: every $Currency in cbc:DocumentCurrencyCode satisfies (count(cac:TaxTotal/xs:decimal(cbc:TaxAmount[@currencyID=$Currency])) eq 1) and (cac:LegalMonetaryTotal/xs:decimal(cbc:TaxInclusiveAmount) = round( (cac:LegalMonetaryTotal/xs:decimal(cbc:TaxExclusiveAmount) + cac:TaxTotal/xs:decimal(cbc:TaxAmount[@currencyID=$Currency])) * 10 * 10) div 100)
fn validate_br_co_15(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-15",
            "[BR-CO-15]-Invoice total amount with VAT (BT-112) = Invoice total amount without VAT (BT-109) + Invoice total VAT amount (BT-110).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: exists(cac:TaxTotal/cac:TaxSubtotal)
fn validate_br_co_18(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-18",
            "[BR-CO-18]-An Invoice shall at least have one VAT breakdown group (BG-23).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (//cac:TaxTotal/cbc:TaxAmount[@currencyID = cbc:DocumentCurrencyCode] and (string-length(substring-after(//cac:TaxTotal/cbc:TaxAmount[@currencyID = cbc:DocumentCurrencyCode],'.'))<=2)) or (not(//cac:TaxTotal/cbc:TaxAmount[@currencyID = cbc:DocumentCurrencyCode]))
fn validate_br_dec_13(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-13",
            "[BR-DEC-13]-The allowed maximum number of decimals for the Invoice total VAT amount (BT-110) is 2.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (//cac:TaxTotal/cbc:TaxAmount[@currencyID = cbc:TaxCurrencyCode] and (string-length(substring-after(//cac:TaxTotal/cbc:TaxAmount[@currencyID = cbc:TaxCurrencyCode],'.'))<=2)) or (not(//cac:TaxTotal/cbc:TaxAmount[@currencyID = cbc:TaxCurrencyCode]))
fn validate_br_dec_15(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-15",
            "[BR-DEC-15]-The allowed maximum number of decimals for the Invoice total VAT amount in accounting currency (BT-111) is 2.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: ((exists(//cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'E']) or exists(//cac:ClassifiedTaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'E'])) and (count(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'E']) = 1)) or (not(//cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'E']) and not(//cac:ClassifiedTaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'E']))
fn validate_br_e_01(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-01",
            "[BR-E-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Exempt from VAT\" shall contain exactly one VAT breakdown (BG-23) with the VAT category code (BT-118) equal to \"Exempt from VAT\".",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'E'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'E'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_e_02(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-02",
            "[BR-E-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Exempt from VAT\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='E'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='E'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_e_03(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-03",
            "[BR-E-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Exempt from VAT\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='E'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='E'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_e_04(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-04",
            "[BR-E-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Exempt from VAT\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: ((exists(//cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'G']) or exists(//cac:ClassifiedTaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'G'])) and (count(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'G']) = 1)) or (not(//cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'G']) and not(//cac:ClassifiedTaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'G']))
fn validate_br_g_01(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-01",
            "[BR-G-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Export outside the EU\" shall contain in the VAT breakdown (BG-23) exactly one VAT category code (BT-118) equal with \"Export outside the EU\".",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'G'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'G'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_g_02(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-02",
            "[BR-G-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Export outside the EU\" shall contain the Seller VAT Identifier (BT-31) or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='G']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='G'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_g_03(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-03",
            "[BR-G-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Export outside the EU\" shall contain the Seller VAT Identifier (BT-31) or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='G']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='G'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_g_04(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-04",
            "[BR-G-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Export outside the EU\" shall contain the Seller VAT Identifier (BT-31) or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: ((exists(//cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'K']) or exists(//cac:ClassifiedTaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'K'])) and (count(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'K']) = 1)) or (not(//cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'K']) and not(//cac:ClassifiedTaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'K']))
fn validate_br_ic_01(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-01",
            "[BR-IC-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Intra-community supply\" shall contain in the VAT breakdown (BG-23) exactly one VAT category code (BT-118) equal with \"Intra-community supply\".",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'K'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID)) and (exists(//cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'K'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT'])
fn validate_br_ic_02(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-02",
            "[BR-IC-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Intra-community supply\" shall contain the Seller VAT Identifier (BT-31) or the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID) = 'K'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID)) and (exists(//cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID) = 'K'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_ic_03(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-03",
            "[BR-IC-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Intra-community supply\" shall contain the Seller VAT Identifier (BT-31) or the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID) = 'K'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID)) and (exists(//cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID) = 'K'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_ic_04(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-04",
            "[BR-IC-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Intra-community supply\" shall contain the Seller VAT Identifier (BT-31) or the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'K'])  and (string-length(cac:Delivery/cbc:ActualDeliveryDate) > 1 or (cac:InvoicePeriod/*))) or (not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'K']))
fn validate_br_ic_11(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-11",
            "[BR-IC-11]-In an Invoice with a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Intra-community supply\" the Actual delivery date (BT-72) or the Invoicing period (BG-14) shall not be blank.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'K']) and (string-length(cac:Delivery/cac:DeliveryLocation/cac:Address/cac:Country/cbc:IdentificationCode) >1)) or (not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'K']))
fn validate_br_ic_12(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-12",
            "[BR-IC-12]-In an Invoice with a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Intra-community supply\" the Deliver to country code (BT-80) shall not be blank.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: ((count(//cac:AllowanceCharge/cac:TaxCategory[normalize-space(cbc:ID) = 'L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) + count(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT'])) > 0 and count(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cbc:ID = 'L']) > 0) or ((count(//cac:AllowanceCharge/cac:TaxCategory[normalize-space(cbc:ID) = 'L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) + count(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT'])) = 0 and count(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) = 0)
fn validate_br_ig_01(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IG-01",
            "[BR-IG-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"IGIC\" shall contain in the VAT breakdown (BG-23) at least one VAT category code (BT-118) equal with \"IGIC\".",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_ig_02(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IG-02",
            "[BR-IG-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"IGIC\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_ig_03(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IG-03",
            "[BR-IG-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"IGIC\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[cbc:ID='L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_ig_04(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IG-04",
            "[BR-IG-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"IGIC\" shall contain the Seller VAT Identifier (BT-31), the Seller Tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: ((count(//cac:AllowanceCharge/cac:TaxCategory[normalize-space(cbc:ID) = 'M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) + count(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT'])) > 0 and count(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cbc:ID = 'M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) > 0) or ((count(//cac:AllowanceCharge/cac:TaxCategory[normalize-space(cbc:ID) = 'M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) + count(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT'])) = 0 and count(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) = 0)
fn validate_br_ip_01(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IP-01",
            "[BR-IP-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"IPSI\" shall contain in the VAT breakdown (BG-23) at least one VAT category code (BT-118) equal with \"IPSI\".",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_ip_02(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IP-02",
            "[BR-IP-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"IPSI\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_ip_03(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IP-03",
            "[BR-IP-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"IPSI\" shall contain the Seller VAT Identifier (BT-31), the Seller Tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_ip_04(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IP-04",
            "[BR-IP-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"IPSI\" shall contain the Seller VAT Identifier (BT-31), the Seller Tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: ((exists(//cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'O']) or exists(//cac:ClassifiedTaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'O'])) and (count(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'O']) = 1)) or (not(//cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'O']) and not(//cac:ClassifiedTaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'O']))
fn validate_br_o_01(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-01",
            "[BR-O-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Not subject to VAT\" shall contain exactly one VAT breakdown group (BG-23) with the VAT category code (BT-118) equal to \"Not subject to VAT\".",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (not(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) and not(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) and not(//cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT'])
fn validate_br_o_02(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-02",
            "[BR-O-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Not subject to VAT\" shall not contain the Seller VAT identifier (BT-31), the Seller tax representative VAT identifier (BT-63) or the Buyer VAT identifier (BT-48).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists((/ubl:Invoice|/cn:CreditNote)/cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID) = 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (not(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) and not(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) and not(//cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists((/ubl:Invoice|/cn:CreditNote)/cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID) = 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_o_03(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-03",
            "[BR-O-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Not subject to VAT\" shall not contain the Seller VAT identifier (BT-31), the Seller tax representative VAT identifier (BT-63) or the Buyer VAT identifier (BT-48).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists((/ubl:Invoice|/cn:CreditNote)/cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID) = 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (not(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) and not(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID) and not(//cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists((/ubl:Invoice|/cn:CreditNote)/cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID) = 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_o_04(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-04",
            "[BR-O-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Not subject to VAT\" shall not contain the Seller VAT identifier (BT-31), the Seller tax representative VAT identifier (BT-63) or the Buyer VAT identifier (BT-48).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'O']) and count(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) != 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) = 0) or not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'O'])
fn validate_br_o_11(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-11",
            "[BR-O-11]-An Invoice that contains a VAT breakdown group (BG-23) with a VAT category code (BT-118) \"Not subject to VAT\" shall not contain other VAT breakdown groups (BG-23).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'O']) and count(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) != 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) = 0) or not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'O'])
fn validate_br_o_12(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-12",
            "[BR-O-12]-An Invoice that contains a VAT breakdown group (BG-23) with a VAT category code (BT-118) \"Not subject to VAT\" shall not contain an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is not \"Not subject to VAT\".",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'O']) and count(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID) != 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) = 0) or not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'O'])
fn validate_br_o_13(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-13",
            "[BR-O-13]-An Invoice that contains a VAT breakdown group (BG-23) with a VAT category code (BT-118) \"Not subject to VAT\" shall not contain Document level allowances (BG-20) where Document level allowance VAT category code (BT-95) is not \"Not subject to VAT\".",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'O']) and count(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID) != 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) = 0) or not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'O'])
fn validate_br_o_14(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-14",
            "[BR-O-14]-An Invoice that contains a VAT breakdown group (BG-23) with a VAT category code (BT-118) \"Not subject to VAT\" shall not contain Document level charges (BG-21) where Document level charge VAT category code (BT-102) is not \"Not subject to VAT\".",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: ((count(//cac:AllowanceCharge/cac:TaxCategory[normalize-space(cbc:ID) = 'S']) + count(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'S'])) > 0 and count(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'S']) > 0) or ((count(//cac:AllowanceCharge/cac:TaxCategory[normalize-space(cbc:ID) = 'S']) + count(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'S'])) = 0 and count(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'S']) = 0)
fn validate_br_s_01(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-01",
            "[BR-S-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Standard rated\" shall contain in the VAT breakdown (BG-23) at least one VAT category code (BT-118) equal with \"Standard rated\".",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'S'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'S']))
fn validate_br_s_02(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-02",
            "[BR-S-02]-An Invoice that contains an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Standard rated\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='S'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='S'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_s_03(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-03",
            "[BR-S-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Standard rated\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='S'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='S'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_s_04(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-04",
            "[BR-S-04]-An Invoice that contains a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Standard rated\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: ((exists(//cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'Z']) or exists(//cac:ClassifiedTaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'Z'])) and (count(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'Z']) = 1)) or (not(//cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'Z']) and not(//cac:ClassifiedTaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID[normalize-space(.) = 'Z']))
fn validate_br_z_01(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-01",
            "[BR-Z-01]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is \"Zero rated\" shall contain in the VAT breakdown (BG-23) exactly one VAT category code (BT-118) equal with \"Zero rated\".",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'Z'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'Z'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_z_02(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-02",
            "[BR-Z-02]-An Invoice that contains an Invoice line where the Invoiced item VAT category code (BT-151) is \"Zero rated\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='Z'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID) or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID) = 'Z'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_z_03(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-03",
            "[BR-Z-03]-An Invoice that contains a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Zero rated\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='Z'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']) and (exists(//cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:CompanyID)or exists(//cac:TaxRepresentativeParty/cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID))) or not(exists(//cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID) = 'Z'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']))
fn validate_br_z_04(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-04",
            "[BR-Z-04]-An Invoice that contains a Document level charge where the Document level charge VAT category code (BT-102) is \"Zero rated\" shall contain the Seller VAT Identifier (BT-31), the Seller tax registration identifier (BT-32) and/or the Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (not(//cbc:IdentificationCode != 'IT') and (//cac:TaxCategory/cbc:ID ='B' or //cac:ClassifiedTaxCategory/cbc:ID = 'B')) or (not(//cac:TaxCategory/cbc:ID ='B' or //cac:ClassifiedTaxCategory/cbc:ID = 'B'))
fn validate_br_b_01(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-B-01",
            "[BR-B-01]-An Invoice where the VAT category code (BT-151, BT-95 or BT-102) is “Split payment” shall be a domestic Italian invoice.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: ((cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory/cbc:ID ='B' or cac:AllowanceCharge/cac:TaxCategory/cbc:ID ='B' or //cac:ClassifiedTaxCategory/cbc:ID = 'B') and (not(cac:TaxTotal/cac:TaxSubtotal/cbc:ID ='S' or cac:AllowanceCharge/cac:TaxCategory/cbc:ID ='S' or //cac:ClassifiedTaxCategory/cbc:ID = 'S'))) or (not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory/cbc:ID ='B' or cac:AllowanceCharge/cac:TaxCategory/cbc:ID ='B' or //cac:ClassifiedTaxCategory/cbc:ID = 'B'))
fn validate_br_b_02(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-B-02",
            "[BR-B-02]-An Invoice that contains an Invoice line (BG-25), a Document level allowance (BG-20) or a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is “Split payment\" shall not contain an invoice line (BG-25), a Document level allowance (BG-20) or  a Document level charge (BG-21) where the VAT category code (BT-151, BT-95 or BT-102) is “Standard rated”.",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: normalize-space(cbc:ID) != ''
fn validate_br_21(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-21",
            "[BR-21]-Each Invoice line (BG-25) shall have an Invoice line identifier (BT-126).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: exists(cbc:InvoicedQuantity) or exists(cbc:CreditedQuantity)
fn validate_br_22(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-22",
            "[BR-22]-Each Invoice line (BG-25) shall have an Invoiced quantity (BT-129).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: exists(cbc:InvoicedQuantity/@unitCode) or exists(cbc:CreditedQuantity/@unitCode)
fn validate_br_23(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-23",
            "[BR-23]-An Invoice line (BG-25) shall have an Invoiced quantity unit of measure code (BT-130).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: exists(cbc:LineExtensionAmount)
fn validate_br_24(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-24",
            "[BR-24]-Each Invoice line (BG-25) shall have an Invoice line net amount (BT-131).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: normalize-space(cac:Item/cbc:Name) != ''
fn validate_br_25(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-25",
            "[BR-25]-Each Invoice line (BG-25) shall contain the Item name (BT-153).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: exists(cac:Price/cbc:PriceAmount)
fn validate_br_26(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-26",
            "[BR-26]-Each Invoice line (BG-25) shall contain the Item net price (BT-146).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: (cac:Price/cbc:PriceAmount) >= 0
fn validate_br_27(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-27",
            "[BR-27]-The Item net price (BT-146) shall NOT be negative.",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: (cac:Price/cac:AllowanceCharge/cbc:BaseAmount) >= 0 or not(exists(cac:Price/cac:AllowanceCharge/cbc:BaseAmount))
fn validate_br_28(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-28",
            "[BR-28]-The Item gross price (BT-148) shall NOT be negative.",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: (cac:Item/cac:ClassifiedTaxCategory[cac:TaxScheme/(normalize-space(upper-case(cbc:ID))='VAT')]/cbc:ID)
fn validate_br_co_04(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-04",
            "[BR-CO-04]-Each Invoice line (BG-25) shall be categorized with an Invoiced item VAT category code (BT-151).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: string-length(substring-after(cbc:LineExtensionAmount,'.'))<=2
fn validate_br_dec_23(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-23",
            "[BR-DEC-23]-The allowed maximum number of decimals for the Invoice line net amount (BT-131) is 2.",
        )));
    }
    Ok(())
}

// Context: //cac:InvoiceLine/cac:AllowanceCharge[cbc:ChargeIndicator = false()] | //cac:CreditNoteLine/cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: exists(cbc:Amount)
fn validate_br_41(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-41",
            "[BR-41]-Each Invoice line allowance (BG-27) shall have an Invoice line allowance amount (BT-136).",
        )));
    }
    Ok(())
}

// Context: //cac:InvoiceLine/cac:AllowanceCharge[cbc:ChargeIndicator = false()] | //cac:CreditNoteLine/cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: exists(cbc:AllowanceChargeReason) or exists(cbc:AllowanceChargeReasonCode)
fn validate_br_42(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-42",
            "[BR-42]-Each Invoice line allowance (BG-27) shall have an Invoice line allowance reason (BT-139) or an Invoice line allowance reason code (BT-140).",
        )));
    }
    Ok(())
}

// Context: //cac:InvoiceLine/cac:AllowanceCharge[cbc:ChargeIndicator = false()] | //cac:CreditNoteLine/cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: true()
fn validate_br_co_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-07",
            "[BR-CO-07]-Invoice line allowance reason code (BT-140) and Invoice line allowance reason (BT-139) shall indicate the same type of allowance reason.",
        )));
    }
    Ok(())
}

// Context: //cac:InvoiceLine/cac:AllowanceCharge[cbc:ChargeIndicator = false()] | //cac:CreditNoteLine/cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: exists(cbc:AllowanceChargeReason) or exists(cbc:AllowanceChargeReasonCode)
fn validate_br_co_23(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-23",
            "[BR-CO-23]-Each Invoice line allowance (BG-27) shall contain an Invoice line allowance reason (BT-139) or an Invoice line allowance reason code (BT-140), or both.",
        )));
    }
    Ok(())
}

// Context: //cac:InvoiceLine/cac:AllowanceCharge[cbc:ChargeIndicator = false()] | //cac:CreditNoteLine/cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: string-length(substring-after(cbc:Amount,'.'))<=2
fn validate_br_dec_24(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-24",
            "[BR-DEC-24]-The allowed maximum number of decimals for the Invoice line allowance amount (BT-136) is 2.",
        )));
    }
    Ok(())
}

// Context: //cac:InvoiceLine/cac:AllowanceCharge[cbc:ChargeIndicator = false()] | //cac:CreditNoteLine/cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: string-length(substring-after(cbc:BaseAmount,'.'))<=2
fn validate_br_dec_25(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-25",
            "[BR-DEC-25]-The allowed maximum number of decimals for the Invoice line allowance base amount (BT-137) is 2.",
        )));
    }
    Ok(())
}

// Context: //cac:InvoiceLine/cac:AllowanceCharge[cbc:ChargeIndicator = true()] | //cac:CreditNoteLine/cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: exists(cbc:Amount)
fn validate_br_43(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-43",
            "[BR-43]-Each Invoice line charge (BG-28) shall have an Invoice line charge amount (BT-141).",
        )));
    }
    Ok(())
}

// Context: //cac:InvoiceLine/cac:AllowanceCharge[cbc:ChargeIndicator = true()] | //cac:CreditNoteLine/cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: exists(cbc:AllowanceChargeReason) or exists(cbc:AllowanceChargeReasonCode)
fn validate_br_44(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-44",
            "[BR-44]-Each Invoice line charge shall have an Invoice line charge reason or an invoice line allowance reason code.",
        )));
    }
    Ok(())
}

// Context: //cac:InvoiceLine/cac:AllowanceCharge[cbc:ChargeIndicator = true()] | //cac:CreditNoteLine/cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: true()
fn validate_br_co_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-08",
            "[BR-CO-08]-Invoice line charge reason code (BT-145) and Invoice line charge reason (BT-144) shall indicate the same type of charge reason.",
        )));
    }
    Ok(())
}

// Context: //cac:InvoiceLine/cac:AllowanceCharge[cbc:ChargeIndicator = true()] | //cac:CreditNoteLine/cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: exists(cbc:AllowanceChargeReason) or exists(cbc:AllowanceChargeReasonCode)
fn validate_br_co_24(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-24",
            "[BR-CO-24]-Each Invoice line charge (BG-28) shall contain an Invoice line charge reason (BT-144) or an Invoice line charge reason code (BT-145), or both.",
        )));
    }
    Ok(())
}

// Context: //cac:InvoiceLine/cac:AllowanceCharge[cbc:ChargeIndicator = true()] | //cac:CreditNoteLine/cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: string-length(substring-after(cbc:Amount,'.'))<=2
fn validate_br_dec_27(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-27",
            "[BR-DEC-27]-The allowed maximum number of decimals for the Invoice line charge amount (BT-141) is 2.",
        )));
    }
    Ok(())
}

// Context: //cac:InvoiceLine/cac:AllowanceCharge[cbc:ChargeIndicator = true()] | //cac:CreditNoteLine/cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: string-length(substring-after(cbc:BaseAmount,'.'))<=2
fn validate_br_dec_28(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-28",
            "[BR-DEC-28]-The allowed maximum number of decimals for the Invoice line charge base amount (BT-142) is 2.",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine/cac:InvoicePeriod | cac:CreditNoteLine/cac:InvoicePeriod
// Test: (exists(cbc:EndDate) and exists(cbc:StartDate) and xs:date(cbc:EndDate) >= xs:date(cbc:StartDate)) or not(exists(cbc:StartDate)) or not(exists(cbc:EndDate))
fn validate_br_30(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-30",
            "[BR-30]-If both Invoice line period start date (BT-134) and Invoice line period end date (BT-135) are given then the Invoice line period end date (BT-135) shall be later or equal to the Invoice line period start date (BT-134).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine/cac:InvoicePeriod | cac:CreditNoteLine/cac:InvoicePeriod
// Test: exists(cbc:StartDate) or exists(cbc:EndDate)
fn validate_br_co_20(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-20",
            "[BR-CO-20]-If Invoice line period (BG-26) is used, the Invoice line period start date (BT-134) or the Invoice line period end date (BT-135) shall be filled, or both.",
        )));
    }
    Ok(())
}

// Context: cac:InvoicePeriod
// Test: (exists(cbc:EndDate) and exists(cbc:StartDate) and xs:date(cbc:EndDate) >= xs:date(cbc:StartDate)) or not(exists(cbc:StartDate)) or not(exists(cbc:EndDate))
fn validate_br_29(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-29",
            "[BR-29]-If both Invoicing period start date (BT-73) and Invoicing period end date (BT-74) are given then the Invoicing period end date (BT-74) shall be later or equal to the Invoicing period start date (BT-73).",
        )));
    }
    Ok(())
}

// Context: cac:InvoicePeriod
// Test: exists(cbc:StartDate) or exists(cbc:EndDate) or (exists(cbc:DescriptionCode) and not(exists(cbc:StartDate)) and not(exists(cbc:EndDate)))
fn validate_br_co_19(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-19",
            "[BR-CO-19]-If Invoicing period (BG-14) is used, the Invoicing period start date (BT-73) or the Invoicing period end date (BT-74) shall be filled, or both.",
        )));
    }
    Ok(())
}

// Context: //cac:AdditionalItemProperty
// Test: exists(cbc:Name) and exists(cbc:Value)
fn validate_br_54(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-54",
            "[BR-54]-Each Item attribute (BG-32) shall contain an Item attribute name (BT-160) and an Item attribute value (BT-161).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine/cac:Item/cac:CommodityClassification/cbc:ItemClassificationCode | cac:CreditNoteLine/cac:Item/cac:CommodityClassification/cbc:ItemClassificationCode
// Test: exists(@listID)
fn validate_br_65(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-65",
            "[BR-65]-The Item classification identifier (BT-158) shall have a Scheme identifier.",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine/cac:Item/cac:StandardItemIdentification/cbc:ID | cac:CreditNoteLine/cac:Item/cac:StandardItemIdentification/cbc:ID
// Test: exists(@schemeID)
fn validate_br_64(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-64",
            "[BR-64]-The Item standard identifier (BT-157) shall have a Scheme identifier.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cbc:Note | /cn:CreditNote/cbc:Note
// Test: (contains(.,'#') and string-length(substring-before(substring-after(.,'#'),'#'))=3 and ( ( contains(' AAA AAB AAC AAD AAE AAF AAG AAI AAJ AAK AAL AAM AAN AAO AAP AAQ AAR AAS AAT AAU AAV AAW AAX AAY AAZ ABA ABB ABC ABD ABE ABF ABG ABH ABI ABJ ABK ABL ABM ABN ABO ABP ABQ ABR ABS ABT ABU ABV ABW ABX ABZ ACA ACB ACC ACD ACE ACF ACG ACH ACI ACJ ACK ACL ACM ACN ACO ACP ACQ ACR ACS ACT ACU ACV ACW ACX ACY ACZ ADA ADB ADC ADD ADE ADF ADG ADH ADI ADJ ADK ADL ADM ADN ADO ADP ADQ ADR ADS ADT ADU ADV ADW ADX ADY ADZ AEA AEB AEC AED AEE AEF AEG AEH AEI AEJ AEK AEL AEM AEN AEO AEP AEQ AER AES AET AEU AEV AEW AEX AEY AEZ AFA AFB AFC AFD AFE AFF AFG AFH AFI AFJ AFK AFL AFM AFN AFO AFP AFQ AFR AFS AFT AFU AFV AFW AFX AFY AFZ AGA AGB AGC AGD AGE AGF AGG AGH AGI AGJ AGK AGL AGM AGN AGO AGP AGQ AGR AGS AGT AGU AGV AGW AGX AGY AGZ AHA AHB AHC AHD AHE AHF AHG AHH AHI AHJ AHK AHL AHM AHN AHO AHP AHQ AHR AHS AHT AHU AHV AHW AHX AHY AHZ AIA AIB AIC AID AIE AIF AIG AIH AII AIJ AIK AIL AIM AIN AIO AIP AIQ AIR AIS AIT AIU AIV AIW AIX AIY AIZ AJA AJB ALC ALD ALE ALF ALG ALH ALI ALJ ALK ALL ALM ALN ALO ALP ALQ ARR ARS AUT AUU AUV AUW AUX AUY AUZ AVA AVB AVC AVD AVE AVF BAG BAH BAI BAJ BAK BAL BAM BAN BAO BAP BAQ BAR BAS BLC BLD BLE BLF BLG BLH BLI BLJ BLK BLL BLM BLN BLO BLP BLQ BLR BLS BLT BLU BLV BLW BLX BLY BLZ BMA BMB BMC BMD BME CCI CEX CHG CIP CLP CLR COI CUR CUS DAR DCL DEL DIN DOC DUT EUR FBC GBL GEN GS7 HAN HAZ ICN IIN IMI IND INS INV IRP ITR ITS LAN LIN LOI MCO MDH MKS ORI OSI PAC PAI PAY PKG PKT PMD PMT PRD PRF PRI PUR QIN QQD QUT RAH REG RET REV RQR SAF SIC SIN SLR SPA SPG SPH SPP SPT SRN SSR SUR TCA TDT TRA TRR TXD WHI ZZZ ',substring-before(substring-after(.,'#'),'#') ) ) )) or not(contains(.,'#')) or not(string-length(substring-before(substring-after(.,'#'),'#'))=3)
fn validate_br_cl_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-08",
            "[BR-CL-08]-Invoiced note subject code shall be coded using UNCL4451",
        )));
    }
    Ok(())
}

// Context: cac:PayeeParty
// Test: exists(cac:PartyName/cbc:Name) and (not(cac:PartyName/cbc:Name = ../cac:AccountingSupplierParty/cac:Party/cac:PartyName/cbc:Name) and not(cac:PartyIdentification/cbc:ID = ../cac:AccountingSupplierParty/cac:Party/cac:PartyIdentification/cbc:ID) )
fn validate_br_17(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-17",
            "[BR-17]-The Payee name (BT-59) shall be provided in the Invoice, if the Payee (BG-10) is different from the Seller (BG-4)",
        )));
    }
    Ok(())
}

// Context: cac:PaymentMeans[cbc:PaymentMeansCode='30' or cbc:PaymentMeansCode='58']/cac:PayeeFinancialAccount
// Test: normalize-space(cbc:ID) != ''
fn validate_br_50(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-50",
            "[BR-50]-A Payment account identifier (BT-84) shall be present if Credit transfer (BG-17) information is provided in the Invoice.",
        )));
    }
    Ok(())
}

// Context: cac:PaymentMeans
// Test: exists(cbc:PaymentMeansCode)
fn validate_br_49(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-49",
            "[BR-49]-A Payment instruction (BG-16) shall specify the Payment means type code (BT-81).",
        )));
    }
    Ok(())
}

// Context: cac:PaymentMeans
// Test: (exists(cac:PayeeFinancialAccount/cbc:ID) and ((normalize-space(cbc:PaymentMeansCode) = '30') or (normalize-space(cbc:PaymentMeansCode) = '58') )) or ((normalize-space(cbc:PaymentMeansCode) != '30') and (normalize-space(cbc:PaymentMeansCode) != '58'))
fn validate_br_61(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-61",
            "[BR-61]-If the Payment means type code (BT-81) means SEPA credit transfer, Local credit transfer or Non-SEPA international credit transfer, the Payment account identifier (BT-84) shall be present.",
        )));
    }
    Ok(())
}

// Context: cac:BillingReference
// Test: exists(cac:InvoiceDocumentReference/cbc:ID)
fn validate_br_55(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-55",
            "[BR-55]-Each Preceding Invoice reference (BG-3) shall contain a Preceding Invoice reference (BT-25).",
        )));
    }
    Ok(())
}

// Context: cac:AccountingSupplierParty
// Test: exists(cac:Party/cac:PartyTaxScheme[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:CompanyID) or exists(cac:Party/cac:PartyIdentification/cbc:ID) or exists(cac:Party/cac:PartyLegalEntity/cbc:CompanyID)
fn validate_br_co_26(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-26",
            "[BR-CO-26]-In order for the buyer to automatically identify a supplier, the Seller identifier (BT-29), the Seller legal registration identifier (BT-30) and/or the Seller VAT identifier (BT-31) shall be present.",
        )));
    }
    Ok(())
}

// Context: cac:AccountingSupplierParty/cac:Party/cbc:EndpointID
// Test: exists(@schemeID)
fn validate_br_62(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-62",
            "[BR-62]-The Seller electronic address (BT-34) shall have a Scheme identifier.",
        )));
    }
    Ok(())
}

// Context: cac:AccountingSupplierParty/cac:Party/cac:PostalAddress
// Test: normalize-space(cac:Country/cbc:IdentificationCode) != ''
fn validate_br_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-09",
            "[BR-09]-The Seller postal address (BG-5) shall contain a Seller country code (BT-40).",
        )));
    }
    Ok(())
}

// Context: cac:TaxRepresentativeParty
// Test: normalize-space(cac:PartyName/cbc:Name) != ''
fn validate_br_18(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-18",
            "[BR-18]-The Seller tax representative name (BT-62) shall be provided in the Invoice, if the Seller (BG-4) has a Seller tax representative party (BG-11)",
        )));
    }
    Ok(())
}

// Context: cac:TaxRepresentativeParty
// Test: exists(cac:PostalAddress)
fn validate_br_19(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-19",
            "[BR-19]-The Seller tax representative postal address (BG-12) shall be provided in the Invoice, if the Seller (BG-4) has a Seller tax representative party (BG-11).",
        )));
    }
    Ok(())
}

// Context: cac:TaxRepresentativeParty
// Test: exists(cac:PartyTaxScheme[cac:TaxScheme/(normalize-space(upper-case(cbc:ID)) = 'VAT')]/cbc:CompanyID)
fn validate_br_56(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-56",
            "[BR-56]-Each Seller tax representative party (BG-11) shall have a Seller tax representative VAT identifier (BT-63).",
        )));
    }
    Ok(())
}

// Context: cac:TaxRepresentativeParty/cac:PostalAddress
// Test: normalize-space(cac:Country/cbc:IdentificationCode) != ''
fn validate_br_20(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-20",
            "[BR-20]-The Seller tax representative postal address (BG-12) shall contain a Tax representative country code (BT-69), if the Seller (BG-4) has a Seller tax representative party (BG-11).",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice/cac:TaxTotal | /cn:CreditNote/cac:TaxTotal
// Test: (xs:decimal(child::cbc:TaxAmount)= round((sum(cac:TaxSubtotal/xs:decimal(cbc:TaxAmount)) * 10 * 10)) div 100) or not(cac:TaxSubtotal)
fn validate_br_co_14(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-14",
            "[BR-CO-14]-Invoice total VAT amount (BT-110) = Σ VAT category tax amount (BT-117).",
        )));
    }
    Ok(())
}

// Context: cac:TaxTotal/cac:TaxSubtotal
// Test: exists(cbc:TaxableAmount)
fn validate_br_45(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-45",
            "[BR-45]-Each VAT breakdown (BG-23) shall have a VAT category taxable amount (BT-116).",
        )));
    }
    Ok(())
}

// Context: cac:TaxTotal/cac:TaxSubtotal
// Test: exists(cbc:TaxAmount)
fn validate_br_46(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-46",
            "[BR-46]-Each VAT breakdown (BG-23) shall have a VAT category tax amount (BT-117).",
        )));
    }
    Ok(())
}

// Context: cac:TaxTotal/cac:TaxSubtotal
// Test: exists(cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:ID)
fn validate_br_47(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-47",
            "[BR-47]-Each VAT breakdown (BG-23) shall be defined through a VAT category code (BT-118).",
        )));
    }
    Ok(())
}

// Context: cac:TaxTotal/cac:TaxSubtotal
// Test: exists(cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/cbc:Percent) or (cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/normalize-space(cbc:ID)='O')
fn validate_br_48(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-48",
            "[BR-48]-Each VAT breakdown (BG-23) shall have a VAT category rate (BT-119), except if the Invoice is not subject to VAT.",
        )));
    }
    Ok(())
}

// Context: cac:TaxTotal/cac:TaxSubtotal
// Test: (round(cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/xs:decimal(cbc:Percent)) = 0 and (round(xs:decimal(cbc:TaxAmount)) = 0)) or (round(cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/xs:decimal(cbc:Percent)) != 0 and ((abs(xs:decimal(cbc:TaxAmount)) - 1 < round(abs(xs:decimal(cbc:TaxableAmount)) * (cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/xs:decimal(cbc:Percent) div 100) * 10 * 10) div 100 ) and (abs(xs:decimal(cbc:TaxAmount)) + 1 > round(abs(xs:decimal(cbc:TaxableAmount)) * (cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/xs:decimal(cbc:Percent) div 100) * 10 * 10) div 100 )))  or (not(exists(cac:TaxCategory[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']/xs:decimal(cbc:Percent))) and (round(xs:decimal(cbc:TaxAmount)) = 0))
fn validate_br_co_17(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-17",
            "[BR-CO-17]-VAT category tax amount (BT-117) = VAT category taxable amount (BT-116) x (VAT category rate (BT-119) / 100), rounded to two decimals.",
        )));
    }
    Ok(())
}

// Context: cac:TaxTotal/cac:TaxSubtotal
// Test: string-length(substring-after(cbc:TaxableAmount,'.'))<=2
fn validate_br_dec_19(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-19",
            "[BR-DEC-19]-The allowed maximum number of decimals for the VAT category taxable amount (BT-116) is 2.",
        )));
    }
    Ok(())
}

// Context: cac:TaxTotal/cac:TaxSubtotal
// Test: string-length(substring-after(cbc:TaxAmount,'.'))<=2
fn validate_br_dec_20(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-DEC-20",
            "[BR-DEC-20]-The allowed maximum number of decimals for the VAT category tax amount (BT-117) is 2.",
        )));
    }
    Ok(())
}

// Context: //cac:PartyTaxScheme[cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: ( contains( ' 1A AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH EL ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS XI YE YT ZA ZM ZW ',substring(cbc:CompanyID,1,2) ) )
fn validate_br_co_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CO-09",
            "[BR-CO-09]-The Seller VAT identifier (BT-31), the Seller tax representative VAT identifier (BT-63) and the Buyer VAT identifier (BT-48) shall have a prefix in accordance with ISO code ISO 3166-1 alpha-2 by which the country of issue may be identified. Nevertheless, Greece may use the prefix ‘EL’.",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'AE'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (exists(//cac:InvoiceLine) and (xs:decimal(../cbc:TaxableAmount) = (sum(../../../cac:InvoiceLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='AE']/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='AE']/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='AE']/xs:decimal(cbc:Amount))))) or (exists(//cac:CreditNoteLine) and (xs:decimal(../cbc:TaxableAmount) = (sum(../../../cac:CreditNoteLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='AE']/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='AE']/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='AE']/xs:decimal(cbc:Amount)))))
fn validate_br_ae_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-08",
            "[BR-AE-08]-In a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Reverse charge\" the VAT category taxable amount (BT-116) shall equal the sum of Invoice line net amounts (BT-131) minus the sum of Document level allowance amounts (BT-92) plus the sum of Document level charge amounts (BT-99) where the VAT category codes (BT-151, BT-95, BT-102) are \"Reverse charge\".",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'AE'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: xs:decimal(../cbc:TaxAmount) = 0
fn validate_br_ae_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-09",
            "[BR-AE-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Reverse charge\" shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'AE'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: exists(cbc:TaxExemptionReason) or (exists(cbc:TaxExemptionReasonCode) )
fn validate_br_ae_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-10",
            "[BR-AE-10]-A VAT breakdown (BG-23) with VAT Category code (BT-118) \"Reverse charge\" shall have a VAT exemption reason code (BT-121), meaning \"Reverse charge\" or the VAT exemption reason text (BT-120) \"Reverse charge\" (or the equivalent standard text in another language).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='AE'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_ae_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-06",
            "[BR-AE-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Reverse charge\" the Document level allowance VAT rate (BT-96) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='AE'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_ae_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-07",
            "[BR-AE-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Reverse charge\" the Document level charge VAT rate (BT-103) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'AE'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT'] | cac:CreditNoteLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'AE'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_ae_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-AE-05",
            "[BR-AE-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Reverse charge\" the Invoiced item VAT rate (BT-152) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'E'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (exists(//cac:InvoiceLine) and (xs:decimal(../cbc:TaxableAmount) = (sum(../../../cac:InvoiceLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='E']/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='E']/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='E']/xs:decimal(cbc:Amount))))) or (exists(//cac:CreditNoteLine) and (xs:decimal(../cbc:TaxableAmount) = (sum(../../../cac:CreditNoteLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='E']/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='E']/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='E']/xs:decimal(cbc:Amount)))))
fn validate_br_e_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-08",
            "[BR-E-08]-In a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Exempt from VAT\" the VAT category taxable amount (BT-116) shall equal the sum of Invoice line net amounts (BT-131) minus the sum of Document level allowance amounts (BT-92) plus the sum of Document level charge amounts (BT-99) where the VAT category codes (BT-151, BT-95, BT-102) are \"Exempt from VAT\".",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'E'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: xs:decimal(../cbc:TaxAmount) = 0
fn validate_br_e_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-09",
            "[BR-E-09]-The VAT category tax amount (BT-117) In a VAT breakdown (BG-23) where the VAT category code (BT-118) equals \"Exempt from VAT\" shall equal 0 (zero).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'E'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: exists(cbc:TaxExemptionReason) or exists(cbc:TaxExemptionReasonCode)
fn validate_br_e_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-10",
            "[BR-E-10]-A VAT breakdown (BG-23) with VAT Category code (BT-118) \"Exempt from VAT\" shall have a VAT exemption reason code (BT-121) or a VAT exemption reason text (BT-120).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='E'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_e_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-06",
            "[BR-E-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Exempt from VAT\", the Document level allowance VAT rate (BT-96) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='E'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_e_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-07",
            "[BR-E-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Exempt from VAT\", the Document level charge VAT rate (BT-103) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'E'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT'] | cac:CreditNoteLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'E'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_e_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-E-05",
            "[BR-E-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Exempt from VAT\", the Invoiced item VAT rate (BT-152) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'G'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (exists(//cac:InvoiceLine) and (xs:decimal(../cbc:TaxableAmount) = (sum(../../../cac:InvoiceLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='G']/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='G']/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='G']/xs:decimal(cbc:Amount))))) or (exists(//cac:CreditNoteLine) and (xs:decimal(../cbc:TaxableAmount) = (sum(../../../cac:CreditNoteLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='G']/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='G']/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='G']/xs:decimal(cbc:Amount)))))
fn validate_br_g_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-08",
            "[BR-G-08]-In a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Export outside the EU\" the VAT category taxable amount (BT-116) shall equal the sum of Invoice line net amounts (BT-131) minus the sum of Document level allowance amounts (BT-92) plus the sum of Document level charge amounts (BT-99) where the VAT category codes (BT-151, BT-95, BT-102) are \"Export outside the EU\".",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'G'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: xs:decimal(../cbc:TaxAmount) = 0
fn validate_br_g_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-09",
            "[BR-G-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Export outside the EU\" shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'G'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: exists(cbc:TaxExemptionReason) or (exists(cbc:TaxExemptionReasonCode) )
fn validate_br_g_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-10",
            "[BR-G-10]-A VAT breakdown (BG-23) with the VAT Category code (BT-118) \"Export outside the EU\" shall have a VAT exemption reason code (BT-121), meaning \"Export outside the EU\" or the VAT exemption reason text (BT-120) \"Export outside the EU\" (or the equivalent standard text in another language).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='G'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_g_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-06",
            "[BR-G-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Export outside the EU\" the Document level allowance VAT rate (BT-96) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='G'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_g_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-07",
            "[BR-G-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Export outside the EU\" the Document level charge VAT rate (BT-103) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'G'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT'] | cac:CreditNoteLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'G'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_g_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-G-05",
            "[BR-G-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Export outside the EU\" the Invoiced item VAT rate (BT-152) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'K'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (exists(//cac:InvoiceLine) and (xs:decimal(../cbc:TaxableAmount) = (sum(../../../cac:InvoiceLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='K']/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='K']/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='K']/xs:decimal(cbc:Amount))))) or (exists(//cac:CreditNoteLine) and (xs:decimal(../cbc:TaxableAmount) = (sum(../../../cac:CreditNoteLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='K']/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='K']/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='K']/xs:decimal(cbc:Amount)))))
fn validate_br_ic_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-08",
            "[BR-IC-08]-In a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Intra-community supply\" the VAT category taxable amount (BT-116) shall equal the sum of Invoice line net amounts (BT-131) minus the sum of Document level allowance amounts (BT-92) plus the sum of Document level charge amounts (BT-99) where the VAT category codes (BT-151, BT-95, BT-102) are \"Intra-community supply\".",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'K'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: xs:decimal(../cbc:TaxAmount) = 0
fn validate_br_ic_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-09",
            "[BR-IC-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Intra-community supply\" shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'K'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: exists(cbc:TaxExemptionReason) or (exists(cbc:TaxExemptionReasonCode) )
fn validate_br_ic_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-10",
            "[BR-IC-10]-A VAT breakdown (BG-23) with the VAT Category code (BT-118) \"Intra-community supply\" shall have a VAT exemption reason code (BT-121), meaning \"Intra-community supply\" or the VAT exemption reason text (BT-120) \"Intra-community supply\" (or the equivalent standard text in another language).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='K'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_ic_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-06",
            "[BR-IC-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Intra-community supply\" the Document level allowance VAT rate (BT-96) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='K'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_ic_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-07",
            "[BR-IC-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Intra-community supply\" the Document level charge VAT rate (BT-103) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'K'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT'] | cac:CreditNoteLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'K'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_ic_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IC-05",
            "[BR-IC-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Intracommunity supply\" the Invoiced item VAT rate (BT-152) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: every $rate in xs:decimal(cbc:Percent) satisfies ((exists(//cac:InvoiceLine) and ((../xs:decimal(cbc:TaxableAmount - 1) < (sum(../../../cac:InvoiceLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='L'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='L'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='L'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)))) and (../xs:decimal(cbc:TaxableAmount + 1) > (sum(../../../cac:InvoiceLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='L'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='L'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='L'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)))))) or (exists(//cac:CreditNoteLine) and ((../xs:decimal(cbc:TaxableAmount - 1) < (sum(../../../cac:CreditNoteLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='L'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='L'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='L'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)))) and (../xs:decimal(cbc:TaxableAmount + 1) > (sum(../../../cac:CreditNoteLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='L'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='L'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='L'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)))))))
fn validate_br_ig_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IG-08",
            "[BR-IG-08]-For each different value of VAT category rate (BT-119) where the VAT category code (BT-118) is \"IGIC\", the VAT category taxable amount (BT-116) in a VAT breakdown (BG-23) shall equal the sum of Invoice line net amounts (BT-131) plus the sum of document level charge amounts (BT-99) minus the sum of document level allowance amounts (BT-92) where the VAT category code (BT-151, BT-102, BT-95) is \"IGIC\" and the VAT rate (BT-152, BT-103, BT-96) equals the VAT category rate (BT-119).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (abs(xs:decimal(../cbc:TaxAmount)) - 1 <  round((abs(xs:decimal(../cbc:TaxableAmount)) * (xs:decimal(cbc:Percent) div 100)) * 10 * 10) div 100 ) and (abs(xs:decimal(../cbc:TaxAmount)) + 1 >  round((abs(xs:decimal(../cbc:TaxableAmount)) * (xs:decimal(cbc:Percent) div 100)) * 10 * 10) div 100 )
fn validate_br_ig_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IG-09",
            "[BR-IG-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where VAT category code (BT-118) is \"IGIC\" shall equal the VAT category taxable amount (BT-116) multiplied by the VAT category rate (BT-119).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: not(cbc:TaxExemptionReason) and not(cbc:TaxExemptionReasonCode)
fn validate_br_ig_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IG-10",
            "[BR-IG-10]-A VAT breakdown (BG-23) with VAT Category code (BT-118) \"IGIC\" shall not have a VAT exemption reason code (BT-121) or VAT exemption reason text (BT-120).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (cbc:Percent) >= 0
fn validate_br_ig_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IG-06",
            "[BR-IG-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"IGIC\" the Document level allowance VAT rate (BT-96) shall be 0 (zero) or greater than zero.",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (cbc:Percent) >= 0
fn validate_br_ig_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IG-07",
            "[BR-IG-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"IGIC\" the Document level charge VAT rate (BT-103) shall be 0 (zero) or greater than zero.",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']| cac:CreditNoteLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'L'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (cbc:Percent) >= 0
fn validate_br_ig_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IG-05",
            "[BR-IG-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"IGIC\" the invoiced item VAT rate (BT-152) shall be 0 (zero) or greater than zero.",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: every $rate in xs:decimal(cbc:Percent) satisfies ((exists(//cac:InvoiceLine) and ((../xs:decimal(cbc:TaxableAmount - 1) < (sum(../../../cac:InvoiceLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='M'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='M'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='M'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)))) and (../xs:decimal(cbc:TaxableAmount + 1) > (sum(../../../cac:InvoiceLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='M'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='M'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='M'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)))))) or (exists(//cac:CreditNoteLine) and ((../xs:decimal(cbc:TaxableAmount - 1) < (sum(../../../cac:CreditNoteLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='M'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='M'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='M'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)))) and (../xs:decimal(cbc:TaxableAmount + 1) > (sum(../../../cac:CreditNoteLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='M'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='M'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='M'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)))))))
fn validate_br_ip_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IP-08",
            "[BR-IP-08]-For each different value of VAT category rate (BT-119) where the VAT category code (BT-118) is \"IPSI\", the VAT category taxable amount (BT-116) in a VAT breakdown (BG-23) shall equal the sum of Invoice line net amounts (BT-131) plus the sum of document level charge amounts (BT-99) minus the sum of document level allowance amounts (BT-92) where the VAT category code (BT-151, BT-102, BT-95) is \"IPSI\" and the VAT rate (BT-152, BT-103, BT-96) equals the VAT category rate (BT-119).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (abs(xs:decimal(../cbc:TaxAmount)) - 1 <  round((abs(xs:decimal(../cbc:TaxableAmount)) * (xs:decimal(cbc:Percent) div 100)) * 10 * 10) div 100 ) and (abs(xs:decimal(../cbc:TaxAmount)) + 1 >  round((abs(xs:decimal(../cbc:TaxableAmount)) * (xs:decimal(cbc:Percent) div 100)) * 10 * 10) div 100 )
fn validate_br_ip_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IP-09",
            "[BR-IP-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where VAT category code (BT-118) is \"IPSI\" shall equal the VAT category taxable amount (BT-116) multiplied by the VAT category rate (BT-119).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: not(cbc:TaxExemptionReason) and not(cbc:TaxExemptionReasonCode)
fn validate_br_ip_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IP-10",
            "[BR-IP-10]-A VAT breakdown (BG-23) with VAT Category code (BT-118) \"IPSI\" shall not have a VAT exemption reason code (BT-121) or VAT exemption reason text (BT-120).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (cbc:Percent) >= 0
fn validate_br_ip_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IP-06",
            "[BR-IP-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"IPSI\" the Document level allowance VAT rate (BT-96) shall be 0 (zero) or greater than zero.",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (cbc:Percent) >= 0
fn validate_br_ip_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IP-07",
            "[BR-IP-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"IPSI\" the Document level charge VAT rate (BT-103) shall be 0 (zero) or greater than zero.",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']| cac:CreditNoteLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'M'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (cbc:Percent) >= 0
fn validate_br_ip_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-IP-05",
            "[BR-IP-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"IPSI\" the Invoiced item VAT rate (BT-152) shall be 0 (zero) or greater than zero.",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (exists(//cac:InvoiceLine) and (xs:decimal(../cbc:TaxableAmount) = (sum(../../../cac:InvoiceLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='O']/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='O']/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='O']/xs:decimal(cbc:Amount))))) or (exists(//cac:CreditNoteLine) and (xs:decimal(../cbc:TaxableAmount) = (sum(../../../cac:CreditNoteLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='O']/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='O']/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='O']/xs:decimal(cbc:Amount)))))
fn validate_br_o_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-08",
            "[BR-O-08]-In a VAT breakdown (BG-23) where the VAT category code (BT-118) is \" Not subject to VAT\" the VAT category taxable amount (BT-116) shall equal the sum of Invoice line net amounts (BT-131) minus the sum of Document level allowance amounts (BT-92) plus the sum of Document level charge amounts (BT-99) where the VAT category codes (BT-151, BT-95, BT-102) are \"Not subject to VAT\".",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: xs:decimal(../cbc:TaxAmount) = 0
fn validate_br_o_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-09",
            "[BR-O-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where the VAT category code (BT-118) is \"Not subject to VAT\" shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: exists(cbc:TaxExemptionReason) or (exists(cbc:TaxExemptionReasonCode) )
fn validate_br_o_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-10",
            "[BR-O-10]-A VAT breakdown (BG-23) with VAT Category code (BT-118) \" Not subject to VAT\" shall have a VAT exemption reason code (BT-121), meaning \" Not subject to VAT\" or a VAT exemption reason text (BT-120) \" Not subject to VAT\" (or the equivalent standard text in another language).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: not(cbc:Percent)
fn validate_br_o_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-06",
            "[BR-O-06]-A Document level allowance (BG-20) where VAT category code (BT-95) is \"Not subject to VAT\" shall not contain a Document level allowance VAT rate (BT-96).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: not(cbc:Percent)
fn validate_br_o_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-07",
            "[BR-O-07]-A Document level charge (BG-21) where the VAT category code (BT-102) is \"Not subject to VAT\" shall not contain a Document level charge VAT rate (BT-103).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT'] | cac:CreditNoteLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'O'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: not(cbc:Percent)
fn validate_br_o_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-O-05",
            "[BR-O-05]-An Invoice line (BG-25) where the VAT category code (BT-151) is \"Not subject to VAT\" shall not contain an Invoiced item VAT rate (BT-152).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'S'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: every $rate in xs:decimal(cbc:Percent) satisfies (((exists(//cac:InvoiceLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID) = 'S'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]) or exists(//cac:AllowanceCharge[cac:TaxCategory/normalize-space(cbc:ID)='S'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate])) and ((../xs:decimal(cbc:TaxableAmount - 1) < (sum(../../../cac:InvoiceLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='S'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='S'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='S'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)))) and (../xs:decimal(cbc:TaxableAmount + 1) > (sum(../../../cac:InvoiceLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='S'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='S'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='S'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)))))) or (exists(//cac:CreditNoteLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID) = 'S'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]) or exists(//cac:AllowanceCharge[cac:TaxCategory/normalize-space(cbc:ID)='S'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate])) and ((../xs:decimal(cbc:TaxableAmount - 1) < (sum(../../../cac:CreditNoteLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='S'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='S'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='S'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)))) and (../xs:decimal(cbc:TaxableAmount + 1) > (sum(../../../cac:CreditNoteLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='S'][cac:Item/cac:ClassifiedTaxCategory/xs:decimal(cbc:Percent) =$rate]/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='S'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='S'][cac:TaxCategory/xs:decimal(cbc:Percent) = $rate]/xs:decimal(cbc:Amount))))))
fn validate_br_s_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-08",
            "[BR-S-08]-For each different value of VAT category rate (BT-119) where the VAT category code (BT-118) is \"Standard rated\", the VAT category taxable amount (BT-116) in a VAT breakdown (BG-23) shall equal the sum of Invoice line net amounts (BT-131) plus the sum of document level charge amounts (BT-99) minus the sum of document level allowance amounts (BT-92) where the VAT category code (BT-151, BT-102, BT-95) is \"Standard rated\" and the VAT rate (BT-152, BT-103, BT-96) equals the VAT category rate (BT-119).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'S'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (abs(xs:decimal(../cbc:TaxAmount)) - 1 <  round((abs(xs:decimal(../cbc:TaxableAmount)) * (xs:decimal(cbc:Percent) div 100)) * 10 * 10) div 100 ) and (abs(xs:decimal(../cbc:TaxAmount)) + 1 >  round((abs(xs:decimal(../cbc:TaxableAmount)) * (xs:decimal(cbc:Percent) div 100)) * 10 * 10) div 100 )
fn validate_br_s_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-09",
            "[BR-S-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where VAT category code (BT-118) is \"Standard rated\" shall equal the VAT category taxable amount (BT-116) multiplied by the VAT category rate (BT-119).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'S'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: not(cbc:TaxExemptionReason) and not(cbc:TaxExemptionReasonCode)
fn validate_br_s_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-10",
            "[BR-S-10]-A VAT breakdown (BG-23) with VAT Category code (BT-118) \"Standard rate\" shall not have a VAT exemption reason code (BT-121) or VAT exemption reason text (BT-120).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='S'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (cbc:Percent) > 0
fn validate_br_s_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-06",
            "[BR-S-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Standard rated\" the Document level allowance VAT rate (BT-96) shall be greater than zero.",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='S'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (cbc:Percent) > 0
fn validate_br_s_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-07",
            "[BR-S-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Standard rated\" the Document level charge VAT rate (BT-103) shall be greater than zero.",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'S'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT'] | cac:CreditNoteLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'S'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (cbc:Percent) > 0
fn validate_br_s_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-S-05",
            "[BR-S-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Standard rated\" the Invoiced item VAT rate (BT-152) shall be greater than zero.",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'Z'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (exists(//cac:InvoiceLine) and (xs:decimal(../cbc:TaxableAmount) = (sum(../../../cac:InvoiceLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='Z']/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='Z']/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='Z']/xs:decimal(cbc:Amount))))) or (exists(//cac:CreditNoteLine) and (xs:decimal(../cbc:TaxableAmount) = (sum(../../../cac:CreditNoteLine[cac:Item/cac:ClassifiedTaxCategory/normalize-space(cbc:ID)='Z']/xs:decimal(cbc:LineExtensionAmount)) + sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=true()][cac:TaxCategory/normalize-space(cbc:ID)='Z']/xs:decimal(cbc:Amount)) - sum(../../../cac:AllowanceCharge[cbc:ChargeIndicator=false()][cac:TaxCategory/normalize-space(cbc:ID)='Z']/xs:decimal(cbc:Amount)))))
fn validate_br_z_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-08",
            "[BR-Z-08]-In a VAT breakdown (BG-23) where VAT category code (BT-118) is \"Zero rated\" the VAT category taxable amount (BT-116) shall equal the sum of Invoice line net amount (BT-131) minus the sum of Document level allowance amounts (BT-92) plus the sum of Document level charge amounts (BT-99) where the VAT category codes (BT-151, BT-95, BT-102) are \"Zero rated\".",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'Z'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: xs:decimal(../cbc:TaxAmount) = 0
fn validate_br_z_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-09",
            "[BR-Z-09]-The VAT category tax amount (BT-117) in a VAT breakdown (BG-23) where VAT category code (BT-118) is \"Zero rated\" shall equal 0 (zero).",
        )));
    }
    Ok(())
}

// Context: /*/cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory[normalize-space(cbc:ID) = 'Z'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: not((cbc:TaxExemptionReason) or (cbc:TaxExemptionReasonCode))
fn validate_br_z_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-10",
            "[BR-Z-10]-A VAT breakdown (BG-23) with VAT Category code (BT-118) \"Zero rated\" shall not have a VAT exemption reason code (BT-121) or VAT exemption reason text (BT-120).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=false()]/cac:TaxCategory[normalize-space(cbc:ID)='Z'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_z_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-06",
            "[BR-Z-06]-In a Document level allowance (BG-20) where the Document level allowance VAT category code (BT-95) is \"Zero rated\" the Document level allowance VAT rate (BT-96) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator=true()]/cac:TaxCategory[normalize-space(cbc:ID)='Z'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_z_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-07",
            "[BR-Z-07]-In a Document level charge (BG-21) where the Document level charge VAT category code (BT-102) is \"Zero rated\" the Document level charge VAT rate (BT-103) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'Z'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT'] | cac:CreditNoteLine/cac:Item/cac:ClassifiedTaxCategory[normalize-space(cbc:ID) = 'Z'][cac:TaxScheme/normalize-space(upper-case(cbc:ID))='VAT']
// Test: (xs:decimal(cbc:Percent) = 0)
fn validate_br_z_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-Z-05",
            "[BR-Z-05]-In an Invoice line (BG-25) where the Invoiced item VAT category code (BT-151) is \"Zero rated\" the Invoiced item VAT rate (BT-152) shall be 0 (zero).",
        )));
    }
    Ok(())
}

// Context: //cac:PostalAddress | //cac:Address
// Test: not(cac:AddressLine) or count(cac:AddressLine) = 1
fn validate_ubl_sr_51(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-51",
            "[UBL-SR-51]-[UBL-SR-51]-An address can only have one third line.",
        )));
    }
    Ok(())
}

// Context: cac:AccountingSupplierParty/cac:Party
// Test: (count(cac:PartyTaxScheme) <= 2)
fn validate_ubl_sr_42(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-42",
            "[UBL-SR-42]-[UBL-SR-42]-Party tax scheme shall occur maximum twice in accounting supplier party",
        )));
    }
    Ok(())
}

// Context: cac:AdditionalDocumentReference
// Test: (count(cbc:DocumentDescription) <= 1)
fn validate_ubl_sr_33(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-33",
            "[UBL-SR-33]-[UBL-SR-33]-Supporting document description shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:AdditionalDocumentReference
// Test: ((cbc:DocumentTypeCode='130') or ((local-name(/*) = 'CreditNote') and (cbc:DocumentTypeCode='50')) or (not(cbc:ID/@schemeID) and not(cbc:DocumentTypeCode)))
fn validate_ubl_sr_43(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-43",
            "[UBL-SR-43]-[UBL-SR-43]-Scheme identifier shall only be used for invoiced object (document type code with value 130 or 50)",
        )));
    }
    Ok(())
}

// Context: //*[ends-with(name(), 'Amount') and not(ends-with(name(),'PriceAmount')) and not(ancestor::cac:Price/cac:AllowanceCharge)]
// Test: string-length(substring-after(.,'.'))<=2
fn validate_ubl_dt_01(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-DT-01",
            "[UBL-DT-01]-[UBL-DT-01]-Amounts shall be decimal up to two fraction digits",
        )));
    }
    Ok(())
}

// Context: //*[ends-with(name(), 'BinaryObject')]
// Test: (@mimeCode)
fn validate_ubl_dt_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-DT-06",
            "[UBL-DT-06]-[UBL-DT-06]-Binary object elements shall contain the mime code attribute",
        )));
    }
    Ok(())
}

// Context: //*[ends-with(name(), 'BinaryObject')]
// Test: (@filename)
fn validate_ubl_dt_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-DT-07",
            "[UBL-DT-07]-[UBL-DT-07]-Binary object elements shall contain the file name attribute",
        )));
    }
    Ok(())
}

// Context: cac:Delivery
// Test: (count(cac:DeliveryParty/cac:PartyName/cbc:Name) <= 1)
fn validate_ubl_sr_25(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-25",
            "[UBL-SR-25]-[UBL-SR-25]-Deliver to party name shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator = false()]
// Test: (count(cbc:AllowanceChargeReason) <= 1)
fn validate_ubl_sr_30(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-30",
            "[UBL-SR-30]-[UBL-SR-30]-Document level allowance reason shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator = true()]
// Test: (count(cbc:AllowanceChargeReason) <= 1)
fn validate_ubl_sr_31(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-31",
            "[UBL-SR-31]-[UBL-SR-31]-Document level charge reason shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(ext:UBLExtensions)
fn validate_ubl_cr_001(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-001",
            "[UBL-CR-001]-[UBL-CR-001]-A UBL invoice should not include extensions",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:UBLVersionID) or cbc:UBLVersionID = '2.1'
fn validate_ubl_cr_002(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-002",
            "[UBL-CR-002]-[UBL-CR-002]-A UBL invoice should not include the UBLVersionID or it should be 2.1",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:ProfileExecutionID)
fn validate_ubl_cr_003(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-003",
            "[UBL-CR-003]-[UBL-CR-003]-A UBL invoice should not include the ProfileExecutionID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:CopyIndicator)
fn validate_ubl_cr_004(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-004",
            "[UBL-CR-004]-[UBL-CR-004]-A UBL invoice should not include the CopyIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:UUID)
fn validate_ubl_cr_005(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-005",
            "[UBL-CR-005]-[UBL-CR-005]-A UBL invoice should not include the UUID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:IssueTime)
fn validate_ubl_cr_006(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-006",
            "[UBL-CR-006]-[UBL-CR-006]-A UBL invoice should not include the IssueTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:PricingCurrencyCode)
fn validate_ubl_cr_007(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-007",
            "[UBL-CR-007]-[UBL-CR-007]-A UBL invoice should not include the PricingCurrencyCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:PaymentCurrencyCode)
fn validate_ubl_cr_008(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-008",
            "[UBL-CR-008]-[UBL-CR-008]-A UBL invoice should not include the PaymentCurrencyCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:PaymentAlternativeCurrencyCode)
fn validate_ubl_cr_009(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-009",
            "[UBL-CR-009]-[UBL-CR-009]-A UBL invoice should not include the PaymentAlternativeCurrencyCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:AccountingCostCode)
fn validate_ubl_cr_010(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-010",
            "[UBL-CR-010]-[UBL-CR-010]-A UBL invoice should not include the AccountingCostCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:LineCountNumeric)
fn validate_ubl_cr_011(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-011",
            "[UBL-CR-011]-[UBL-CR-011]-A UBL invoice should not include the LineCountNumeric",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:InvoicePeriod/cbc:StartTime)
fn validate_ubl_cr_012(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-012",
            "[UBL-CR-012]-[UBL-CR-012]-A UBL invoice should not include the InvoicePeriod StartTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:InvoicePeriod/cbc:EndTime)
fn validate_ubl_cr_013(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-013",
            "[UBL-CR-013]-[UBL-CR-013]-A UBL invoice should not include the InvoicePeriod EndTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:InvoicePeriod/cbc:DurationMeasure)
fn validate_ubl_cr_014(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-014",
            "[UBL-CR-014]-[UBL-CR-014]-A UBL invoice should not include the InvoicePeriod DurationMeasure",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:InvoicePeriod/cbc:Description)
fn validate_ubl_cr_015(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-015",
            "[UBL-CR-015]-[UBL-CR-015]-A UBL invoice should not include the InvoicePeriod Description",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OrderReference/cbc:CopyIndicator)
fn validate_ubl_cr_016(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-016",
            "[UBL-CR-016]-[UBL-CR-016]-A UBL invoice should not include the OrderReference CopyIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OrderReference/cbc:UUID)
fn validate_ubl_cr_017(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-017",
            "[UBL-CR-017]-[UBL-CR-017]-A UBL invoice should not include the OrderReference UUID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OrderReference/cbc:IssueDate)
fn validate_ubl_cr_018(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-018",
            "[UBL-CR-018]-[UBL-CR-018]-A UBL invoice should not include the OrderReference IssueDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OrderReference/cbc:IssueTime)
fn validate_ubl_cr_019(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-019",
            "[UBL-CR-019]-[UBL-CR-019]-A UBL invoice should not include the OrderReference IssueTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OrderReference/cbc:CustomerReference)
fn validate_ubl_cr_020(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-020",
            "[UBL-CR-020]-[UBL-CR-020]-A UBL invoice should not include the OrderReference CustomerReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OrderReference/cbc:OrderTypeCode)
fn validate_ubl_cr_021(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-021",
            "[UBL-CR-021]-[UBL-CR-021]-A UBL invoice should not include the OrderReference OrderTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OrderReference/cac:DocumentReference)
fn validate_ubl_cr_022(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-022",
            "[UBL-CR-022]-[UBL-CR-022]-A UBL invoice should not include the OrderReference DocumentReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cbc:CopyIndicator)
fn validate_ubl_cr_023(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-023",
            "[UBL-CR-023]-[UBL-CR-023]-A UBL invoice should not include the BillingReference CopyIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cbc:UUID)
fn validate_ubl_cr_024(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-024",
            "[UBL-CR-024]-[UBL-CR-024]-A UBL invoice should not include the BillingReference UUID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cbc:IssueTime)
fn validate_ubl_cr_025(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-025",
            "[UBL-CR-025]-[UBL-CR-025]-A UBL invoice should not include the BillingReference IssueTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cbc:DocumentTypeCode)
fn validate_ubl_cr_026(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-026",
            "[UBL-CR-026]-[UBL-CR-026]-A UBL invoice should not include the BillingReference DocumentTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cbc:DocumentType)
fn validate_ubl_cr_027(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-027",
            "[UBL-CR-027]-[UBL-CR-027]-A UBL invoice should not include the BillingReference DocumentType",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cbc:XPath)
fn validate_ubl_cr_028(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-028",
            "[UBL-CR-028]-[UBL-CR-028]-A UBL invoice should not include the BillingReference Xpath",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cbc:LanguageID)
fn validate_ubl_cr_029(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-029",
            "[UBL-CR-029]-[UBL-CR-029]-A UBL invoice should not include the BillingReference LanguageID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cbc:LocaleCode)
fn validate_ubl_cr_030(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-030",
            "[UBL-CR-030]-[UBL-CR-030]-A UBL invoice should not include the BillingReference LocaleCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cbc:VersionID)
fn validate_ubl_cr_031(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-031",
            "[UBL-CR-031]-[UBL-CR-031]-A UBL invoice should not include the BillingReference VersionID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cbc:DocumentStatusCode)
fn validate_ubl_cr_032(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-032",
            "[UBL-CR-032]-[UBL-CR-032]-A UBL invoice should not include the BillingReference DocumentStatusCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cbc:DocumentDescription)
fn validate_ubl_cr_033(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-033",
            "[UBL-CR-033]-[UBL-CR-033]-A UBL invoice should not include the BillingReference DocumenDescription",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cac:Attachment)
fn validate_ubl_cr_034(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-034",
            "[UBL-CR-034]-[UBL-CR-034]-A UBL invoice should not include the BillingReference Attachment",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cac:ValidityPeriod)
fn validate_ubl_cr_035(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-035",
            "[UBL-CR-035]-[UBL-CR-035]-A UBL invoice should not include the BillingReference ValidityPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cac:IssuerParty)
fn validate_ubl_cr_036(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-036",
            "[UBL-CR-036]-[UBL-CR-036]-A UBL invoice should not include the BillingReference IssuerParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:InvoiceDocumentReference/cac:ResultOfVerification)
fn validate_ubl_cr_037(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-037",
            "[UBL-CR-037]-[UBL-CR-037]-A UBL invoice should not include the BillingReference ResultOfVerification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:SelfBilledInvoiceDocumentReference)
fn validate_ubl_cr_038(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-038",
            "[UBL-CR-038]-[UBL-CR-038]-A UBL invoice should not include the BillingReference SelfBilledInvoiceDocumentReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:CreditNoteDocumentReference)
fn validate_ubl_cr_039(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-039",
            "[UBL-CR-039]-[UBL-CR-039]-A UBL invoice should not include the BillingReference CreditNoteDocumentReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:SelfBilledCreditNoteDocumentReference)
fn validate_ubl_cr_040(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-040",
            "[UBL-CR-040]-[UBL-CR-040]-A UBL invoice should not include the BillingReference SelfBilledCreditNoteDocumentReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:DebitNoteDocumentReference)
fn validate_ubl_cr_041(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-041",
            "[UBL-CR-041]-[UBL-CR-041]-A UBL invoice should not include the BillingReference DebitNoteDocumentReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:ReminderDocumentReference)
fn validate_ubl_cr_042(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-042",
            "[UBL-CR-042]-[UBL-CR-042]-A UBL invoice should not include the BillingReference ReminderDocumentReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:AdditionalDocumentReference)
fn validate_ubl_cr_043(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-043",
            "[UBL-CR-043]-[UBL-CR-043]-A UBL invoice should not include the BillingReference AdditionalDocumentReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BillingReference/cac:BillingReferenceLine)
fn validate_ubl_cr_044(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-044",
            "[UBL-CR-044]-[UBL-CR-044]-A UBL invoice should not include the BillingReference BillingReferenceLine",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cbc:CopyIndicator)
fn validate_ubl_cr_045(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-045",
            "[UBL-CR-045]-[UBL-CR-045]-A UBL invoice should not include the DespatchDocumentReference CopyIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cbc:UUID)
fn validate_ubl_cr_046(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-046",
            "[UBL-CR-046]-[UBL-CR-046]-A UBL invoice should not include the DespatchDocumentReference UUID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cbc:IssueDate)
fn validate_ubl_cr_047(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-047",
            "[UBL-CR-047]-[UBL-CR-047]-A UBL invoice should not include the DespatchDocumentReference IssueDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cbc:IssueTime)
fn validate_ubl_cr_048(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-048",
            "[UBL-CR-048]-[UBL-CR-048]-A UBL invoice should not include the DespatchDocumentReference IssueTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cbc:DocumentTypeCode)
fn validate_ubl_cr_049(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-049",
            "[UBL-CR-049]-[UBL-CR-049]-A UBL invoice should not include the DespatchDocumentReference DocumentTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cbc:DocumentType)
fn validate_ubl_cr_050(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-050",
            "[UBL-CR-050]-[UBL-CR-050]-A UBL invoice should not include the DespatchDocumentReference DocumentType",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cbc:XPath)
fn validate_ubl_cr_051(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-051",
            "[UBL-CR-051]-[UBL-CR-051]-A UBL invoice should not include the DespatchDocumentReference Xpath",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cbc:LanguageID)
fn validate_ubl_cr_052(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-052",
            "[UBL-CR-052]-[UBL-CR-052]-A UBL invoice should not include the DespatchDocumentReference LanguageID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cbc:LocaleCode)
fn validate_ubl_cr_053(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-053",
            "[UBL-CR-053]-[UBL-CR-053]-A UBL invoice should not include the DespatchDocumentReference LocaleCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cbc:VersionID)
fn validate_ubl_cr_054(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-054",
            "[UBL-CR-054]-[UBL-CR-054]-A UBL invoice should not include the DespatchDocumentReference VersionID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cbc:DocumentStatusCode)
fn validate_ubl_cr_055(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-055",
            "[UBL-CR-055]-[UBL-CR-055]-A UBL invoice should not include the DespatchDocumentReference DocumentStatusCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cbc:DocumentDescription)
fn validate_ubl_cr_056(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-056",
            "[UBL-CR-056]-[UBL-CR-056]-A UBL invoice should not include the DespatchDocumentReference DocumentDescription",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cac:Attachment)
fn validate_ubl_cr_057(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-057",
            "[UBL-CR-057]-[UBL-CR-057]-A UBL invoice should not include the DespatchDocumentReference Attachment",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cac:ValidityPeriod)
fn validate_ubl_cr_058(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-058",
            "[UBL-CR-058]-[UBL-CR-058]-A UBL invoice should not include the DespatchDocumentReference ValidityPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cac:IssuerParty)
fn validate_ubl_cr_059(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-059",
            "[UBL-CR-059]-[UBL-CR-059]-A UBL invoice should not include the DespatchDocumentReference IssuerParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DespatchDocumentReference/cac:ResultOfVerification)
fn validate_ubl_cr_060(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-060",
            "[UBL-CR-060]-[UBL-CR-060]-A UBL invoice should not include the DespatchDocumentReference ResultOfVerification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cbc:CopyIndicator)
fn validate_ubl_cr_061(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-061",
            "[UBL-CR-061]-[UBL-CR-061]-A UBL invoice should not include the ReceiptDocumentReference CopyIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cbc:UUID)
fn validate_ubl_cr_062(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-062",
            "[UBL-CR-062]-[UBL-CR-062]-A UBL invoice should not include the ReceiptDocumentReference UUID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cbc:IssueDate)
fn validate_ubl_cr_063(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-063",
            "[UBL-CR-063]-[UBL-CR-063]-A UBL invoice should not include the ReceiptDocumentReference IssueDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cbc:IssueTime)
fn validate_ubl_cr_064(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-064",
            "[UBL-CR-064]-[UBL-CR-064]-A UBL invoice should not include the ReceiptDocumentReference IssueTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cbc:DocumentTypeCode)
fn validate_ubl_cr_065(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-065",
            "[UBL-CR-065]-[UBL-CR-065]-A UBL invoice should not include the ReceiptDocumentReference DocumentTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cbc:DocumentType)
fn validate_ubl_cr_066(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-066",
            "[UBL-CR-066]-[UBL-CR-066]-A UBL invoice should not include the ReceiptDocumentReference DocumentType",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cbc:XPath)
fn validate_ubl_cr_067(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-067",
            "[UBL-CR-067]-[UBL-CR-067]-A UBL invoice should not include the ReceiptDocumentReference Xpath",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cbc:LanguageID)
fn validate_ubl_cr_068(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-068",
            "[UBL-CR-068]-[UBL-CR-068]-A UBL invoice should not include the ReceiptDocumentReference LanguageID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cbc:LocaleCode)
fn validate_ubl_cr_069(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-069",
            "[UBL-CR-069]-[UBL-CR-069]-A UBL invoice should not include the ReceiptDocumentReference LocaleCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cbc:VersionID)
fn validate_ubl_cr_070(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-070",
            "[UBL-CR-070]-[UBL-CR-070]-A UBL invoice should not include the ReceiptDocumentReference VersionID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cbc:DocumentStatusCode)
fn validate_ubl_cr_071(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-071",
            "[UBL-CR-071]-[UBL-CR-071]-A UBL invoice should not include the ReceiptDocumentReference DocumentStatusCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cbc:DocumentDescription)
fn validate_ubl_cr_072(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-072",
            "[UBL-CR-072]-[UBL-CR-072]-A UBL invoice should not include the ReceiptDocumentReference DocumentDescription",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cac:Attachment)
fn validate_ubl_cr_073(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-073",
            "[UBL-CR-073]-[UBL-CR-073]-A UBL invoice should not include the ReceiptDocumentReference Attachment",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cac:ValidityPeriod)
fn validate_ubl_cr_074(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-074",
            "[UBL-CR-074]-[UBL-CR-074]-A UBL invoice should not include the ReceiptDocumentReference ValidityPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cac:IssuerParty)
fn validate_ubl_cr_075(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-075",
            "[UBL-CR-075]-[UBL-CR-075]-A UBL invoice should not include the ReceiptDocumentReference IssuerParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ReceiptDocumentReference/cac:ResultOfVerification)
fn validate_ubl_cr_076(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-076",
            "[UBL-CR-076]-[UBL-CR-076]-A UBL invoice should not include the ReceiptDocumentReference ResultOfVerification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:StatementDocumentReference)
fn validate_ubl_cr_077(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-077",
            "[UBL-CR-077]-[UBL-CR-077]-A UBL invoice should not include the StatementDocumentReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cbc:CopyIndicator)
fn validate_ubl_cr_078(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-078",
            "[UBL-CR-078]-[UBL-CR-078]-A UBL invoice should not include the OriginatorDocumentReference CopyIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cbc:UUID)
fn validate_ubl_cr_079(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-079",
            "[UBL-CR-079]-[UBL-CR-079]-A UBL invoice should not include the OriginatorDocumentReference UUID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cbc:IssueDate)
fn validate_ubl_cr_080(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-080",
            "[UBL-CR-080]-[UBL-CR-080]-A UBL invoice should not include the OriginatorDocumentReference IssueDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cbc:IssueTime)
fn validate_ubl_cr_081(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-081",
            "[UBL-CR-081]-[UBL-CR-081]-A UBL invoice should not include the OriginatorDocumentReference IssueTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cbc:DocumentTypeCode)
fn validate_ubl_cr_082(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-082",
            "[UBL-CR-082]-[UBL-CR-082]-A UBL invoice should not include the OriginatorDocumentReference DocumentTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cbc:DocumentType)
fn validate_ubl_cr_083(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-083",
            "[UBL-CR-083]-[UBL-CR-083]-A UBL invoice should not include the OriginatorDocumentReference DocumentType",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cbc:XPath)
fn validate_ubl_cr_084(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-084",
            "[UBL-CR-084]-[UBL-CR-084]-A UBL invoice should not include the OriginatorDocumentReference Xpath",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cbc:LanguageID)
fn validate_ubl_cr_085(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-085",
            "[UBL-CR-085]-[UBL-CR-085]-A UBL invoice should not include the OriginatorDocumentReference LanguageID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cbc:LocaleCode)
fn validate_ubl_cr_086(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-086",
            "[UBL-CR-086]-[UBL-CR-086]-A UBL invoice should not include the OriginatorDocumentReference LocaleCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cbc:VersionID)
fn validate_ubl_cr_087(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-087",
            "[UBL-CR-087]-[UBL-CR-087]-A UBL invoice should not include the OriginatorDocumentReference VersionID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cbc:DocumentStatusCode)
fn validate_ubl_cr_088(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-088",
            "[UBL-CR-088]-[UBL-CR-088]-A UBL invoice should not include the OriginatorDocumentReference DocumentStatusCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cbc:DocumentDescription)
fn validate_ubl_cr_089(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-089",
            "[UBL-CR-089]-[UBL-CR-089]-A UBL invoice should not include the OriginatorDocumentReference DocumentDescription",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cac:Attachment)
fn validate_ubl_cr_090(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-090",
            "[UBL-CR-090]-[UBL-CR-090]-A UBL invoice should not include the OriginatorDocumentReference Attachment",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cac:ValidityPeriod)
fn validate_ubl_cr_091(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-091",
            "[UBL-CR-091]-[UBL-CR-091]-A UBL invoice should not include the OriginatorDocumentReference ValidityPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cac:IssuerParty)
fn validate_ubl_cr_092(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-092",
            "[UBL-CR-092]-[UBL-CR-092]-A UBL invoice should not include the OriginatorDocumentReference IssuerParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:OriginatorDocumentReference/cac:ResultOfVerification)
fn validate_ubl_cr_093(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-093",
            "[UBL-CR-093]-[UBL-CR-093]-A UBL invoice should not include the OriginatorDocumentReference ResultOfVerification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cbc:CopyIndicator)
fn validate_ubl_cr_094(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-094",
            "[UBL-CR-094]-[UBL-CR-094]-A UBL invoice should not include the ContractDocumentReference CopyIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cbc:UUID)
fn validate_ubl_cr_095(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-095",
            "[UBL-CR-095]-[UBL-CR-095]-A UBL invoice should not include the ContractDocumentReference UUID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cbc:IssueDate)
fn validate_ubl_cr_096(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-096",
            "[UBL-CR-096]-[UBL-CR-096]-A UBL invoice should not include the ContractDocumentReference IssueDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cbc:IssueTime)
fn validate_ubl_cr_097(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-097",
            "[UBL-CR-097]-[UBL-CR-097]-A UBL invoice should not include the ContractDocumentReference IssueTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cbc:DocumentTypeCode)
fn validate_ubl_cr_098(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-098",
            "[UBL-CR-098]-[UBL-CR-098]-A UBL invoice should not include the ContractDocumentReference DocumentTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cbc:DocumentType)
fn validate_ubl_cr_099(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-099",
            "[UBL-CR-099]-[UBL-CR-099]-A UBL invoice should not include the ContractDocumentReference DocumentType",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cbc:XPath)
fn validate_ubl_cr_100(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-100",
            "[UBL-CR-100]-[UBL-CR-100]-A UBL invoice should not include the ContractDocumentReference Xpath",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cbc:LanguageID)
fn validate_ubl_cr_101(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-101",
            "[UBL-CR-101]-[UBL-CR-101]-A UBL invoice should not include the ContractDocumentReference LanguageID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cbc:LocaleCode)
fn validate_ubl_cr_102(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-102",
            "[UBL-CR-102]-[UBL-CR-102]-A UBL invoice should not include the ContractDocumentReference LocaleCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cbc:VersionID)
fn validate_ubl_cr_103(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-103",
            "[UBL-CR-103]-[UBL-CR-103]-A UBL invoice should not include the ContractDocumentReference VersionID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cbc:DocumentStatusCode)
fn validate_ubl_cr_104(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-104",
            "[UBL-CR-104]-[UBL-CR-104]-A UBL invoice should not include the ContractDocumentReference DocumentStatusCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cbc:DocumentDescription)
fn validate_ubl_cr_105(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-105",
            "[UBL-CR-105]-[UBL-CR-105]-A UBL invoice should not include the ContractDocumentReference DocumentDescription",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cac:Attachment)
fn validate_ubl_cr_106(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-106",
            "[UBL-CR-106]-[UBL-CR-106]-A UBL invoice should not include the ContractDocumentReference Attachment",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cac:ValidityPeriod)
fn validate_ubl_cr_107(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-107",
            "[UBL-CR-107]-[UBL-CR-107]-A UBL invoice should not include the ContractDocumentReference ValidityPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cac:IssuerParty)
fn validate_ubl_cr_108(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-108",
            "[UBL-CR-108]-[UBL-CR-108]-A UBL invoice should not include the ContractDocumentReference IssuerParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ContractDocumentReference/cac:ResultOfVerification)
fn validate_ubl_cr_109(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-109",
            "[UBL-CR-109]-[UBL-CR-109]-A UBL invoice should not include the ContractDocumentReference ResultOfVerification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cbc:CopyIndicator)
fn validate_ubl_cr_110(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-110",
            "[UBL-CR-110]-[UBL-CR-110]-A UBL invoice should not include the AdditionalDocumentReference CopyIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cbc:UUID)
fn validate_ubl_cr_111(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-111",
            "[UBL-CR-111]-[UBL-CR-111]-A UBL invoice should not include the AdditionalDocumentReference UUID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cbc:IssueDate)
fn validate_ubl_cr_112(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-112",
            "[UBL-CR-112]-[UBL-CR-112]-A UBL invoice should not include the AdditionalDocumentReference IssueDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cbc:IssueTime)
fn validate_ubl_cr_113(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-113",
            "[UBL-CR-113]-[UBL-CR-113]-A UBL invoice should not include the AdditionalDocumentReference IssueTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cbc:DocumentType)
fn validate_ubl_cr_114(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-114",
            "[UBL-CR-114]-[UBL-CR-114]-A UBL invoice should not include the AdditionalDocumentReference DocumentType",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cbc:XPath)
fn validate_ubl_cr_115(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-115",
            "[UBL-CR-115]-[UBL-CR-115]-A UBL invoice should not include the AdditionalDocumentReference Xpath",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cbc:LanguageID)
fn validate_ubl_cr_116(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-116",
            "[UBL-CR-116]-[UBL-CR-116]-A UBL invoice should not include the AdditionalDocumentReference LanguageID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cbc:LocaleCode)
fn validate_ubl_cr_117(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-117",
            "[UBL-CR-117]-[UBL-CR-117]-A UBL invoice should not include the AdditionalDocumentReference LocaleCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cbc:VersionID)
fn validate_ubl_cr_118(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-118",
            "[UBL-CR-118]-[UBL-CR-118]-A UBL invoice should not include the AdditionalDocumentReference VersionID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cbc:DocumentStatusCode)
fn validate_ubl_cr_119(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-119",
            "[UBL-CR-119]-[UBL-CR-119]-A UBL invoice should not include the AdditionalDocumentReference DocumentStatusCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cac:Attachment/cac:ExternalReference/cbc:DocumentHash)
fn validate_ubl_cr_121(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-121",
            "[UBL-CR-121]-[UBL-CR-121]-A UBL invoice should not include the AdditionalDocumentReference Attachment External DocumentHash",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cac:Attachment/cac:ExternalReference/cbc:HashAlgorithmMethod)
fn validate_ubl_cr_122(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-122",
            "[UBL-CR-122]-[UBL-CR-122]-A UBL invoice should not include the AdditionalDocumentReference Attachment External HashAlgorithmMethod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cac:Attachment/cac:ExternalReference/cbc:ExpiryDate)
fn validate_ubl_cr_123(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-123",
            "[UBL-CR-123]-[UBL-CR-123]-A UBL invoice should not include the AdditionalDocumentReference Attachment External ExpiryDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cac:Attachment/cac:ExternalReference/cbc:ExpiryTime)
fn validate_ubl_cr_124(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-124",
            "[UBL-CR-124]-[UBL-CR-124]-A UBL invoice should not include the AdditionalDocumentReference Attachment External ExpiryTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cac:Attachment/cac:ExternalReference/cbc:MimeCode)
fn validate_ubl_cr_125(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-125",
            "[UBL-CR-125]-[UBL-CR-125]-A UBL invoice should not include the AdditionalDocumentReference Attachment External MimeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cac:Attachment/cac:ExternalReference/cbc:FormatCode)
fn validate_ubl_cr_126(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-126",
            "[UBL-CR-126]-[UBL-CR-126]-A UBL invoice should not include the AdditionalDocumentReference Attachment External FormatCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cac:Attachment/cac:ExternalReference/cbc:EncodingCode)
fn validate_ubl_cr_127(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-127",
            "[UBL-CR-127]-[UBL-CR-127]-A UBL invoice should not include the AdditionalDocumentReference Attachment External EncodingCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cac:Attachment/cac:ExternalReference/cbc:CharacterSetCode)
fn validate_ubl_cr_128(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-128",
            "[UBL-CR-128]-[UBL-CR-128]-A UBL invoice should not include the AdditionalDocumentReference Attachment External CharacterSetCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cac:Attachment/cac:ExternalReference/cbc:FileName)
fn validate_ubl_cr_129(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-129",
            "[UBL-CR-129]-[UBL-CR-129]-A UBL invoice should not include the AdditionalDocumentReference Attachment External FileName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cac:Attachment/cac:ExternalReference/cbc:Description)
fn validate_ubl_cr_130(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-130",
            "[UBL-CR-130]-[UBL-CR-130]-A UBL invoice should not include the AdditionalDocumentReference Attachment External Descriprion",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cac:ValidityPeriod)
fn validate_ubl_cr_131(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-131",
            "[UBL-CR-131]-[UBL-CR-131]-A UBL invoice should not include the AdditionalDocumentReference ValidityPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cac:IssuerParty)
fn validate_ubl_cr_132(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-132",
            "[UBL-CR-132]-[UBL-CR-132]-A UBL invoice should not include the AdditionalDocumentReference IssuerParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cac:ResultOfVerification)
fn validate_ubl_cr_133(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-133",
            "[UBL-CR-133]-[UBL-CR-133]-A UBL invoice should not include the AdditionalDocumentReference ResultOfVerification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ProjectReference/cbc:UUID)
fn validate_ubl_cr_134(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-134",
            "[UBL-CR-134]-[UBL-CR-134]-A UBL invoice should not include the ProjectReference UUID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ProjectReference/cbc:IssueDate)
fn validate_ubl_cr_135(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-135",
            "[UBL-CR-135]-[UBL-CR-135]-A UBL invoice should not include the ProjectReference IssueDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:ProjectReference/cac:WorkPhaseReference)
fn validate_ubl_cr_136(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-136",
            "[UBL-CR-136]-[UBL-CR-136]-A UBL invoice should not include the ProjectReference WorkPhaseReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Signature)
fn validate_ubl_cr_137(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-137",
            "[UBL-CR-137]-[UBL-CR-137]-A UBL invoice should not include the Signature",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cbc:CustomerAssignedAccountID)
fn validate_ubl_cr_138(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-138",
            "[UBL-CR-138]-[UBL-CR-138]-A UBL invoice should not include the AccountingSupplierParty CustomerAssignedAccountID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cbc:AdditionalAccountID)
fn validate_ubl_cr_139(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-139",
            "[UBL-CR-139]-[UBL-CR-139]-A UBL invoice should not include the AccountingSupplierParty AdditionalAccountID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cbc:DataSendingCapability)
fn validate_ubl_cr_140(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-140",
            "[UBL-CR-140]-[UBL-CR-140]-A UBL invoice should not include the AccountingSupplierParty DataSendingCapability",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cbc:MarkCareIndicator)
fn validate_ubl_cr_141(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-141",
            "[UBL-CR-141]-[UBL-CR-141]-A UBL invoice should not include the AccountingSupplierParty Party MarkCareIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cbc:MarkAttentionIndicator)
fn validate_ubl_cr_142(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-142",
            "[UBL-CR-142]-[UBL-CR-142]-A UBL invoice should not include the AccountingSupplierParty Party MarkAttentionIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cbc:WebsiteURI)
fn validate_ubl_cr_143(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-143",
            "[UBL-CR-143]-[UBL-CR-143]-A UBL invoice should not include the AccountingSupplierParty Party WebsiteURI",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cbc:LogoReferenceID)
fn validate_ubl_cr_144(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-144",
            "[UBL-CR-144]-[UBL-CR-144]-A UBL invoice should not include the AccountingSupplierParty Party LogoReferenceID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cbc:IndustryClassificationCode)
fn validate_ubl_cr_145(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-145",
            "[UBL-CR-145]-[UBL-CR-145]-A UBL invoice should not include the AccountingSupplierParty Party IndustryClassificationCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:Language)
fn validate_ubl_cr_146(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-146",
            "[UBL-CR-146]-[UBL-CR-146]-A UBL invoice should not include the AccountingSupplierParty Party Language",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:ID)
fn validate_ubl_cr_147(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-147",
            "[UBL-CR-147]-[UBL-CR-147]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:AddressTypeCode)
fn validate_ubl_cr_148(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-148",
            "[UBL-CR-148]-[UBL-CR-148]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress AddressTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:AddressFormatCode)
fn validate_ubl_cr_149(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-149",
            "[UBL-CR-149]-[UBL-CR-149]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress AddressFormatCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:Postbox)
fn validate_ubl_cr_150(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-150",
            "[UBL-CR-150]-[UBL-CR-150]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress Postbox",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:Floor)
fn validate_ubl_cr_151(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-151",
            "[UBL-CR-151]-[UBL-CR-151]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress Floor",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:Room)
fn validate_ubl_cr_152(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-152",
            "[UBL-CR-152]-[UBL-CR-152]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress Room",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:BlockName)
fn validate_ubl_cr_153(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-153",
            "[UBL-CR-153]-[UBL-CR-153]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress BlockName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:BuildingName)
fn validate_ubl_cr_154(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-154",
            "[UBL-CR-154]-[UBL-CR-154]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress BuildingName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:BuildingNumber)
fn validate_ubl_cr_155(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-155",
            "[UBL-CR-155]-[UBL-CR-155]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress BuildingNumber",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:InhouseMail)
fn validate_ubl_cr_156(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-156",
            "[UBL-CR-156]-[UBL-CR-156]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress InhouseMail",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:Department)
fn validate_ubl_cr_157(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-157",
            "[UBL-CR-157]-[UBL-CR-157]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress Department",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:MarkAttention)
fn validate_ubl_cr_158(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-158",
            "[UBL-CR-158]-[UBL-CR-158]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress MarkAttention",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:MarkCare)
fn validate_ubl_cr_159(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-159",
            "[UBL-CR-159]-[UBL-CR-159]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress MarkCare",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:PlotIdentification)
fn validate_ubl_cr_160(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-160",
            "[UBL-CR-160]-[UBL-CR-160]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress PlotIdentification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:CitySubdivisionName)
fn validate_ubl_cr_161(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-161",
            "[UBL-CR-161]-[UBL-CR-161]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress CitySubdivisionName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:CountrySubentityCode)
fn validate_ubl_cr_162(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-162",
            "[UBL-CR-162]-[UBL-CR-162]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress CountrySubentityCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:Region)
fn validate_ubl_cr_163(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-163",
            "[UBL-CR-163]-[UBL-CR-163]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress Region",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:District)
fn validate_ubl_cr_164(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-164",
            "[UBL-CR-164]-[UBL-CR-164]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress District",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cbc:TimezoneOffset)
fn validate_ubl_cr_165(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-165",
            "[UBL-CR-165]-[UBL-CR-165]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress TimezoneOffset",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cac:Country/cbc:Name)
fn validate_ubl_cr_166(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-166",
            "[UBL-CR-166]-[UBL-CR-166]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress Country Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PostalAddress/cac:LocationCoordinate)
fn validate_ubl_cr_167(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-167",
            "[UBL-CR-167]-[UBL-CR-167]-A UBL invoice should not include the AccountingSupplierParty Party PostalAddress LocationCoordinate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PhysicalLocation)
fn validate_ubl_cr_168(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-168",
            "[UBL-CR-168]-[UBL-CR-168]-A UBL invoice should not include the AccountingSupplierParty Party PhysicalLocation",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:RegistrationName)
fn validate_ubl_cr_169(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-169",
            "[UBL-CR-169]-[UBL-CR-169]-A UBL invoice should not include the AccountingSupplierParty Party PartyTaxScheme RegistrationName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:TaxLevelCode)
fn validate_ubl_cr_170(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-170",
            "[UBL-CR-170]-[UBL-CR-170]-A UBL invoice should not include the AccountingSupplierParty Party PartyTaxScheme TaxLevelCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:ExemptionReasonCode)
fn validate_ubl_cr_171(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-171",
            "[UBL-CR-171]-[UBL-CR-171]-A UBL invoice should not include the AccountingSupplierParty Party PartyTaxScheme ExemptionReasonCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cbc:ExemptionReason)
fn validate_ubl_cr_172(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-172",
            "[UBL-CR-172]-[UBL-CR-172]-A UBL invoice should not include the AccountingSupplierParty Party PartyTaxScheme ExemptionReason",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cac:RegistrationAddress)
fn validate_ubl_cr_173(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-173",
            "[UBL-CR-173]-[UBL-CR-173]-A UBL invoice should not include the AccountingSupplierParty Party PartyTaxScheme RegistrationAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cac:TaxScheme/cbc:Name)
fn validate_ubl_cr_174(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-174",
            "[UBL-CR-174]-[UBL-CR-174]-A UBL invoice should not include the AccountingSupplierParty Party PartyTaxScheme TaxScheme Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cac:TaxScheme/cbc:TaxTypeCode)
fn validate_ubl_cr_175(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-175",
            "[UBL-CR-175]-[UBL-CR-175]-A UBL invoice should not include the AccountingSupplierParty Party PartyTaxScheme TaxScheme TaxTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cac:TaxScheme/cbc:CurrencyCode)
fn validate_ubl_cr_176(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-176",
            "[UBL-CR-176]-[UBL-CR-176]-A UBL invoice should not include the AccountingSupplierParty Party PartyTaxScheme TaxScheme CurrencyCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme/cac:TaxScheme/cac:JurisdictionRegionAddress)
fn validate_ubl_cr_177(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-177",
            "[UBL-CR-177]-[UBL-CR-177]-A UBL invoice should not include the AccountingSupplierParty Party PartyTaxScheme TaxScheme JurisdictionRegionAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:RegistrationDate)
fn validate_ubl_cr_178(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-178",
            "[UBL-CR-178]-[UBL-CR-178]-A UBL invoice should not include the AccountingSupplierParty Party PartyLegalEntity RegistrationDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:RegistrationExpirationDate)
fn validate_ubl_cr_179(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-179",
            "[UBL-CR-179]-[UBL-CR-179]-A UBL invoice should not include the AccountingSupplierParty Party PartyLegalEntity RegistrationExpirationDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:CompanyLegalFormCode)
fn validate_ubl_cr_180(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-180",
            "[UBL-CR-180]-[UBL-CR-180]-A UBL invoice should not include the AccountingSupplierParty Party PartyLegalEntity CompanyLegalFormCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:SoleProprietorshipIndicator)
fn validate_ubl_cr_181(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-181",
            "[UBL-CR-181]-[UBL-CR-181]-A UBL invoice should not include the AccountingSupplierParty Party PartyLegalEntity SoleProprietorshipIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:CompanyLiquidationStatusCode)
fn validate_ubl_cr_182(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-182",
            "[UBL-CR-182]-[UBL-CR-182]-A UBL invoice should not include the AccountingSupplierParty Party PartyLegalEntity CompanyLiquidationStatusCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:CorporateStockAmount)
fn validate_ubl_cr_183(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-183",
            "[UBL-CR-183]-[UBL-CR-183]-A UBL invoice should not include the AccountingSupplierParty Party PartyLegalEntity CorporateStockAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:FullyPaidSharesIndicator)
fn validate_ubl_cr_184(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-184",
            "[UBL-CR-184]-[UBL-CR-184]-A UBL invoice should not include the AccountingSupplierParty Party PartyLegalEntity FullyPaidSharesIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cac:RegistrationAddress)
fn validate_ubl_cr_185(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-185",
            "[UBL-CR-185]-[UBL-CR-185]-A UBL invoice should not include the AccountingSupplierParty Party PartyLegalEntity RegistrationAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cac:CorporateRegistrationScheme)
fn validate_ubl_cr_186(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-186",
            "[UBL-CR-186]-[UBL-CR-186]-A UBL invoice should not include the AccountingSupplierParty Party PartyLegalEntity CorporateRegistrationScheme",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cac:HeadOfficeParty)
fn validate_ubl_cr_187(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-187",
            "[UBL-CR-187]-[UBL-CR-187]-A UBL invoice should not include the AccountingSupplierParty Party PartyLegalEntity HeadOfficeParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cac:ShareholderParty)
fn validate_ubl_cr_188(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-188",
            "[UBL-CR-188]-[UBL-CR-188]-A UBL invoice should not include the AccountingSupplierParty Party PartyLegalEntity ShareholderParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:Contact/cbc:ID)
fn validate_ubl_cr_189(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-189",
            "[UBL-CR-189]-[UBL-CR-189]-A UBL invoice should not include the AccountingSupplierParty Party Contact ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:Contact/cbc:Telefax)
fn validate_ubl_cr_190(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-190",
            "[UBL-CR-190]-[UBL-CR-190]-A UBL invoice should not include the AccountingSupplierParty Party Contact Telefax",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:Contact/cbc:Note)
fn validate_ubl_cr_191(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-191",
            "[UBL-CR-191]-[UBL-CR-191]-A UBL invoice should not include the AccountingSupplierParty Party Contact Note",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:Contact/cac:OtherCommunication)
fn validate_ubl_cr_192(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-192",
            "[UBL-CR-192]-[UBL-CR-192]-A UBL invoice should not include the AccountingSupplierParty Party Contact OtherCommunication",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:Person)
fn validate_ubl_cr_193(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-193",
            "[UBL-CR-193]-[UBL-CR-193]-A UBL invoice should not include the AccountingSupplierParty Party Person",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:AgentParty)
fn validate_ubl_cr_194(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-194",
            "[UBL-CR-194]-[UBL-CR-194]-A UBL invoice should not include the AccountingSupplierParty Party AgentParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:ServiceProviderParty)
fn validate_ubl_cr_195(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-195",
            "[UBL-CR-195]-[UBL-CR-195]-A UBL invoice should not include the AccountingSupplierParty Party ServiceProviderParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:PowerOfAttorney)
fn validate_ubl_cr_196(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-196",
            "[UBL-CR-196]-[UBL-CR-196]-A UBL invoice should not include the AccountingSupplierParty Party PowerOfAttorney",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:Party/cac:FinancialAccount)
fn validate_ubl_cr_197(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-197",
            "[UBL-CR-197]-[UBL-CR-197]-A UBL invoice should not include the AccountingSupplierParty Party FinancialAccount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:DespatchContact)
fn validate_ubl_cr_198(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-198",
            "[UBL-CR-198]-[UBL-CR-198]-A UBL invoice should not include the AccountingSupplierParty DespatchContact",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:AccountingContact)
fn validate_ubl_cr_199(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-199",
            "[UBL-CR-199]-[UBL-CR-199]-A UBL invoice should not include the AccountingSupplierParty AccountingContact",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingSupplierParty/cac:SellerContact)
fn validate_ubl_cr_200(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-200",
            "[UBL-CR-200]-[UBL-CR-200]-A UBL invoice should not include the AccountingSupplierParty SellerContact",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cbc:CustomerAssignedAccountID)
fn validate_ubl_cr_201(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-201",
            "[UBL-CR-201]-[UBL-CR-201]-A UBL invoice should not include the AccountingCustomerParty CustomerAssignedAccountID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cbc:SupplierAssignedAccountID)
fn validate_ubl_cr_202(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-202",
            "[UBL-CR-202]-[UBL-CR-202]-A UBL invoice should not include the AccountingCustomerParty SupplierAssignedAccountID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cbc:AdditionalAccountID)
fn validate_ubl_cr_203(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-203",
            "[UBL-CR-203]-[UBL-CR-203]-A UBL invoice should not include the AccountingCustomerParty AdditionalAccountID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cbc:MarkCareIndicator)
fn validate_ubl_cr_204(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-204",
            "[UBL-CR-204]-[UBL-CR-204]-A UBL invoice should not include the AccountingCustomerParty Party MarkCareIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cbc:MarkAttentionIndicator)
fn validate_ubl_cr_205(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-205",
            "[UBL-CR-205]-[UBL-CR-205]-A UBL invoice should not include the AccountingCustomerParty Party MarkAttentionIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cbc:WebsiteURI)
fn validate_ubl_cr_206(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-206",
            "[UBL-CR-206]-[UBL-CR-206]-A UBL invoice should not include the AccountingCustomerParty Party WebsiteURI",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cbc:LogoReferenceID)
fn validate_ubl_cr_207(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-207",
            "[UBL-CR-207]-[UBL-CR-207]-A UBL invoice should not include the AccountingCustomerParty Party LogoReferenceID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cbc:IndustryClassificationCode)
fn validate_ubl_cr_208(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-208",
            "[UBL-CR-208]-[UBL-CR-208]-A UBL invoice should not include the AccountingCustomerParty Party IndustryClassificationCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:Language)
fn validate_ubl_cr_209(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-209",
            "[UBL-CR-209]-[UBL-CR-209]-A UBL invoice should not include the AccountingCustomerParty Party Language",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:ID)
fn validate_ubl_cr_210(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-210",
            "[UBL-CR-210]-[UBL-CR-210]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:AddressTypeCode)
fn validate_ubl_cr_211(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-211",
            "[UBL-CR-211]-[UBL-CR-211]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress AddressTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:AddressFormatCode)
fn validate_ubl_cr_212(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-212",
            "[UBL-CR-212]-[UBL-CR-212]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress AddressFormatCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:Postbox)
fn validate_ubl_cr_213(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-213",
            "[UBL-CR-213]-[UBL-CR-213]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress Postbox",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:Floor)
fn validate_ubl_cr_214(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-214",
            "[UBL-CR-214]-[UBL-CR-214]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress Floor",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:Room)
fn validate_ubl_cr_215(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-215",
            "[UBL-CR-215]-[UBL-CR-215]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress Room",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:BlockName)
fn validate_ubl_cr_216(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-216",
            "[UBL-CR-216]-[UBL-CR-216]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress BlockName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:BuildingName)
fn validate_ubl_cr_217(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-217",
            "[UBL-CR-217]-[UBL-CR-217]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress BuildingName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:BuildingNumber)
fn validate_ubl_cr_218(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-218",
            "[UBL-CR-218]-[UBL-CR-218]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress BuildingNumber",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:InhouseMail)
fn validate_ubl_cr_219(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-219",
            "[UBL-CR-219]-[UBL-CR-219]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress InhouseMail",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:Department)
fn validate_ubl_cr_220(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-220",
            "[UBL-CR-220]-[UBL-CR-220]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress Department",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:MarkAttention)
fn validate_ubl_cr_221(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-221",
            "[UBL-CR-221]-[UBL-CR-221]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress MarkAttention",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:MarkCare)
fn validate_ubl_cr_222(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-222",
            "[UBL-CR-222]-[UBL-CR-222]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress MarkCare",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:PlotIdentification)
fn validate_ubl_cr_223(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-223",
            "[UBL-CR-223]-[UBL-CR-223]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress PlotIdentification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:CitySubdivisionName)
fn validate_ubl_cr_224(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-224",
            "[UBL-CR-224]-[UBL-CR-224]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress CitySubdivisionName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:CountrySubentityCode)
fn validate_ubl_cr_225(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-225",
            "[UBL-CR-225]-[UBL-CR-225]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress CountrySubentityCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:Region)
fn validate_ubl_cr_226(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-226",
            "[UBL-CR-226]-[UBL-CR-226]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress Region",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:District)
fn validate_ubl_cr_227(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-227",
            "[UBL-CR-227]-[UBL-CR-227]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress District",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cbc:TimezoneOffset)
fn validate_ubl_cr_228(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-228",
            "[UBL-CR-228]-[UBL-CR-228]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress TimezoneOffset",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cac:Country/cbc:Name)
fn validate_ubl_cr_229(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-229",
            "[UBL-CR-229]-[UBL-CR-229]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress Country Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PostalAddress/cac:LocationCoordinate)
fn validate_ubl_cr_230(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-230",
            "[UBL-CR-230]-[UBL-CR-230]-A UBL invoice should not include the AccountingCustomerParty Party PostalAddress LocationCoordinate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PhysicalLocation)
fn validate_ubl_cr_231(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-231",
            "[UBL-CR-231]-[UBL-CR-231]-A UBL invoice should not include the AccountingCustomerParty Party PhysicalLocation",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme/cbc:RegistrationName)
fn validate_ubl_cr_232(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-232",
            "[UBL-CR-232]-[UBL-CR-232]-A UBL invoice should not include the AccountingCustomerParty Party PartyTaxScheme RegistrationName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme/cbc:TaxLevelCode)
fn validate_ubl_cr_233(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-233",
            "[UBL-CR-233]-[UBL-CR-233]-A UBL invoice should not include the AccountingCustomerParty Party PartyTaxScheme TaxLevelCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme/cbc:ExemptionReasonCode)
fn validate_ubl_cr_234(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-234",
            "[UBL-CR-234]-[UBL-CR-234]-A UBL invoice should not include the AccountingCustomerParty Party PartyTaxScheme ExemptionReasonCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme/cbc:ExemptionReason)
fn validate_ubl_cr_235(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-235",
            "[UBL-CR-235]-[UBL-CR-235]-A UBL invoice should not include the AccountingCustomerParty Party PartyTaxScheme ExemptionReason",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme/cac:RegistrationAddress)
fn validate_ubl_cr_236(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-236",
            "[UBL-CR-236]-[UBL-CR-236]-A UBL invoice should not include the AccountingCustomerParty Party PartyTaxScheme RegistrationAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme/cac:TaxScheme/cbc:Name)
fn validate_ubl_cr_237(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-237",
            "[UBL-CR-237]-[UBL-CR-237]-A UBL invoice should not include the AccountingCustomerParty Party PartyTaxScheme TaxScheme Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme/cac:TaxScheme/cbc:TaxTypeCode)
fn validate_ubl_cr_238(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-238",
            "[UBL-CR-238]-[UBL-CR-238]-A UBL invoice should not include the AccountingCustomerParty Party PartyTaxScheme TaxScheme TaxTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme/cac:TaxScheme/cbc:CurrencyCode)
fn validate_ubl_cr_239(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-239",
            "[UBL-CR-239]-[UBL-CR-239]-A UBL invoice should not include the AccountingCustomerParty Party PartyTaxScheme TaxScheme CurrencyCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme/cac:TaxScheme/cac:JurisdictionRegionAddress)
fn validate_ubl_cr_240(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-240",
            "[UBL-CR-240]-[UBL-CR-240]-A UBL invoice should not include the AccountingCustomerParty Party PartyTaxScheme TaxScheme JurisdictionRegionAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:RegistrationDate)
fn validate_ubl_cr_241(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-241",
            "[UBL-CR-241]-[UBL-CR-241]-A UBL invoice should not include the AccountingCustomerParty Party PartyLegalEntity RegistrationDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:RegistrationExpirationDate)
fn validate_ubl_cr_242(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-242",
            "[UBL-CR-242]-[UBL-CR-242]-A UBL invoice should not include the AccountingCustomerParty Party PartyLegalEntity RegistrationExpirationDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:CompanyLegalFormCode)
fn validate_ubl_cr_243(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-243",
            "[UBL-CR-243]-[UBL-CR-243]-A UBL invoice should not include the AccountingCustomerParty Party PartyLegalEntity CompanyLegalFormCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:CompanyLegalForm)
fn validate_ubl_cr_244(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-244",
            "[UBL-CR-244]-[UBL-CR-244]-A UBL invoice should not include the AccountingCustomerParty Party PartyLegalEntity CompanyLegalForm",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:SoleProprietorshipIndicator)
fn validate_ubl_cr_245(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-245",
            "[UBL-CR-245]-[UBL-CR-245]-A UBL invoice should not include the AccountingCustomerParty Party PartyLegalEntity SoleProprietorshipIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:CompanyLiquidationStatusCode)
fn validate_ubl_cr_246(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-246",
            "[UBL-CR-246]-[UBL-CR-246]-A UBL invoice should not include the AccountingCustomerParty Party PartyLegalEntity CompanyLiquidationStatusCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:CorporateStockAmount)
fn validate_ubl_cr_247(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-247",
            "[UBL-CR-247]-[UBL-CR-247]-A UBL invoice should not include the AccountingCustomerParty Party PartyLegalEntity CorporateStockAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:FullyPaidSharesIndicator)
fn validate_ubl_cr_248(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-248",
            "[UBL-CR-248]-[UBL-CR-248]-A UBL invoice should not include the AccountingCustomerParty Party PartyLegalEntity FullyPaidSharesIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cac:RegistrationAddress)
fn validate_ubl_cr_249(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-249",
            "[UBL-CR-249]-[UBL-CR-249]-A UBL invoice should not include the AccountingCustomerParty Party PartyLegalEntity RegistrationAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cac:CorporateRegistrationScheme)
fn validate_ubl_cr_250(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-250",
            "[UBL-CR-250]-[UBL-CR-250]-A UBL invoice should not include the AccountingCustomerParty Party PartyLegalEntity CorporateRegistrationScheme",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cac:HeadOfficeParty)
fn validate_ubl_cr_251(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-251",
            "[UBL-CR-251]-[UBL-CR-251]-A UBL invoice should not include the AccountingCustomerParty Party PartyLegalEntity HeadOfficeParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cac:ShareholderParty)
fn validate_ubl_cr_252(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-252",
            "[UBL-CR-252]-[UBL-CR-252]-A UBL invoice should not include the AccountingCustomerParty Party PartyLegalEntity ShareholderParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:Contact/cbc:ID)
fn validate_ubl_cr_253(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-253",
            "[UBL-CR-253]-[UBL-CR-253]-A UBL invoice should not include the AccountingCustomerParty Party Contact ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:Contact/cbc:Telefax)
fn validate_ubl_cr_254(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-254",
            "[UBL-CR-254]-[UBL-CR-254]-A UBL invoice should not include the AccountingCustomerParty Party Contact Telefax",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:Contact/cbc:Note)
fn validate_ubl_cr_255(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-255",
            "[UBL-CR-255]-[UBL-CR-255]-A UBL invoice should not include the AccountingCustomerParty Party Contact Note",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:Contact/cac:OtherCommunication)
fn validate_ubl_cr_256(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-256",
            "[UBL-CR-256]-[UBL-CR-256]-A UBL invoice should not include the AccountingCustomerParty Party Contact OtherCommunication",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:Person)
fn validate_ubl_cr_257(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-257",
            "[UBL-CR-257]-[UBL-CR-257]-A UBL invoice should not include the AccountingCustomerParty Party Person",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:AgentParty)
fn validate_ubl_cr_258(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-258",
            "[UBL-CR-258]-[UBL-CR-258]-A UBL invoice should not include the AccountingCustomerParty Party AgentParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:ServiceProviderParty)
fn validate_ubl_cr_259(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-259",
            "[UBL-CR-259]-[UBL-CR-259]-A UBL invoice should not include the AccountingCustomerParty Party ServiceProviderParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:PowerOfAttorney)
fn validate_ubl_cr_260(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-260",
            "[UBL-CR-260]-[UBL-CR-260]-A UBL invoice should not include the AccountingCustomerParty Party PowerOfAttorney",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:Party/cac:FinancialAccount)
fn validate_ubl_cr_261(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-261",
            "[UBL-CR-261]-[UBL-CR-261]-A UBL invoice should not include the AccountingCustomerParty Party FinancialAccount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:DeliveryContact)
fn validate_ubl_cr_262(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-262",
            "[UBL-CR-262]-[UBL-CR-262]-A UBL invoice should not include the AccountingCustomerParty DeliveryContact",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:AccountingContact)
fn validate_ubl_cr_263(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-263",
            "[UBL-CR-263]-[UBL-CR-263]-A UBL invoice should not include the AccountingCustomerParty AccountingContact",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AccountingCustomerParty/cac:BuyerContact)
fn validate_ubl_cr_264(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-264",
            "[UBL-CR-264]-[UBL-CR-264]-A UBL invoice should not include the AccountingCustomerParty BuyerContact",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cbc:MarkCareIndicator)
fn validate_ubl_cr_265(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-265",
            "[UBL-CR-265]-[UBL-CR-265]-A UBL invoice should not include the PayeeParty MarkCareIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cbc:MarkAttentionIndicator)
fn validate_ubl_cr_266(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-266",
            "[UBL-CR-266]-[UBL-CR-266]-A UBL invoice should not include the PayeeParty MarkAttentionIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cbc:WebsiteURI)
fn validate_ubl_cr_267(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-267",
            "[UBL-CR-267]-[UBL-CR-267]-A UBL invoice should not include the PayeeParty WebsiteURI",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cbc:LogoReferenceID)
fn validate_ubl_cr_268(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-268",
            "[UBL-CR-268]-[UBL-CR-268]-A UBL invoice should not include the PayeeParty LogoReferenceID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cbc:EndpointID)
fn validate_ubl_cr_269(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-269",
            "[UBL-CR-269]-[UBL-CR-269]-A UBL invoice should not include the PayeeParty EndpointID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cbc:IndustryClassificationCode)
fn validate_ubl_cr_270(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-270",
            "[UBL-CR-270]-[UBL-CR-270]-A UBL invoice should not include the PayeeParty IndustryClassificationCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:Language)
fn validate_ubl_cr_271(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-271",
            "[UBL-CR-271]-[UBL-CR-271]-A UBL invoice should not include the PayeeParty Language",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PostalAddress)
fn validate_ubl_cr_272(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-272",
            "[UBL-CR-272]-[UBL-CR-272]-A UBL invoice should not include the PayeeParty PostalAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PhysicalLocation)
fn validate_ubl_cr_273(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-273",
            "[UBL-CR-273]-[UBL-CR-273]-A UBL invoice should not include the PayeeParty PhysicalLocation",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyTaxScheme)
fn validate_ubl_cr_274(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-274",
            "[UBL-CR-274]-[UBL-CR-274]-A UBL invoice should not include the PayeeParty PartyTaxScheme",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyLegalEntity/cbc:RegistrationName)
fn validate_ubl_cr_275(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-275",
            "[UBL-CR-275]-[UBL-CR-275]-A UBL invoice should not include the PayeeParty PartyLegalEntity RegistrationName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyLegalEntity/cbc:RegistrationDate)
fn validate_ubl_cr_276(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-276",
            "[UBL-CR-276]-[UBL-CR-276]-A UBL invoice should not include the PayeeParty PartyLegalEntity RegistrationDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyLegalEntity/cbc:RegistrationExpirationDate)
fn validate_ubl_cr_277(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-277",
            "[UBL-CR-277]-[UBL-CR-277]-A UBL invoice should not include the PayeeParty PartyLegalEntity RegistrationExpirationDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyLegalEntity/cbc:CompanyLegalFormCode)
fn validate_ubl_cr_278(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-278",
            "[UBL-CR-278]-[UBL-CR-278]-A UBL invoice should not include the PayeeParty PartyLegalEntity CompanyLegalFormCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyLegalEntity/cbc:CompanyLegalForm)
fn validate_ubl_cr_279(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-279",
            "[UBL-CR-279]-[UBL-CR-279]-A UBL invoice should not include the PayeeParty PartyLegalEntity CompanyLegalForm",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyLegalEntity/cbc:SoleProprietorshipIndicator)
fn validate_ubl_cr_280(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-280",
            "[UBL-CR-280]-[UBL-CR-280]-A UBL invoice should not include the PayeeParty PartyLegalEntity SoleProprietorshipIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyLegalEntity/cbc:CompanyLiquidationStatusCode)
fn validate_ubl_cr_281(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-281",
            "[UBL-CR-281]-[UBL-CR-281]-A UBL invoice should not include the PayeeParty PartyLegalEntity CompanyLiquidationStatusCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyLegalEntity/cbc:CorporateStockAmount)
fn validate_ubl_cr_282(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-282",
            "[UBL-CR-282]-[UBL-CR-282]-A UBL invoice should not include the PayeeParty PartyLegalEntity CorporateStockAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyLegalEntity/cbc:FullyPaidSharesIndicator)
fn validate_ubl_cr_283(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-283",
            "[UBL-CR-283]-[UBL-CR-283]-A UBL invoice should not include the PayeeParty PartyLegalEntity FullyPaidSharesIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyLegalEntity/cac:RegistrationAddress)
fn validate_ubl_cr_284(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-284",
            "[UBL-CR-284]-[UBL-CR-284]-A UBL invoice should not include the PayeeParty PartyLegalEntity RegistrationAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyLegalEntity/cac:CorporateRegistrationScheme)
fn validate_ubl_cr_285(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-285",
            "[UBL-CR-285]-[UBL-CR-285]-A UBL invoice should not include the PayeeParty PartyLegalEntity CorporateRegistrationScheme",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyLegalEntity/cac:HeadOfficeParty)
fn validate_ubl_cr_286(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-286",
            "[UBL-CR-286]-[UBL-CR-286]-A UBL invoice should not include the PayeeParty PartyLegalEntity HeadOfficeParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PartyLegalEntity/cac:ShareholderParty)
fn validate_ubl_cr_287(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-287",
            "[UBL-CR-287]-[UBL-CR-287]-A UBL invoice should not include the PayeeParty PartyLegalEntity ShareholderParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:Contact)
fn validate_ubl_cr_288(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-288",
            "[UBL-CR-288]-[UBL-CR-288]-A UBL invoice should not include the PayeeParty Contact",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:Person)
fn validate_ubl_cr_289(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-289",
            "[UBL-CR-289]-[UBL-CR-289]-A UBL invoice should not include the PayeeParty Person",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:AgentParty)
fn validate_ubl_cr_290(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-290",
            "[UBL-CR-290]-[UBL-CR-290]-A UBL invoice should not include the PayeeParty AgentParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:ServiceProviderParty)
fn validate_ubl_cr_291(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-291",
            "[UBL-CR-291]-[UBL-CR-291]-A UBL invoice should not include the PayeeParty ServiceProviderParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:PowerOfAttorney)
fn validate_ubl_cr_292(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-292",
            "[UBL-CR-292]-[UBL-CR-292]-A UBL invoice should not include the PayeeParty PowerOfAttorney",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PayeeParty/cac:FinancialAccount)
fn validate_ubl_cr_293(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-293",
            "[UBL-CR-293]-[UBL-CR-293]-A UBL invoice should not include the PayeeParty FinancialAccount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:BuyerCustomerParty)
fn validate_ubl_cr_294(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-294",
            "[UBL-CR-294]-[UBL-CR-294]-A UBL invoice should not include the BuyerCustomerParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:SellerSupplierParty)
fn validate_ubl_cr_295(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-295",
            "[UBL-CR-295]-[UBL-CR-295]-A UBL invoice should not include the SellerSupplierParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cbc:MarkCareIndicator)
fn validate_ubl_cr_296(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-296",
            "[UBL-CR-296]-[UBL-CR-296]-A UBL invoice should not include the TaxRepresentativeParty MarkCareIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cbc:MarkAttentionIndicator)
fn validate_ubl_cr_297(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-297",
            "[UBL-CR-297]-[UBL-CR-297]-A UBL invoice should not include the TaxRepresentativeParty MarkAttentionIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cbc:WebsiteURI)
fn validate_ubl_cr_298(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-298",
            "[UBL-CR-298]-[UBL-CR-298]-A UBL invoice should not include the TaxRepresentativeParty WebsiteURI",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cbc:LogoReferenceID)
fn validate_ubl_cr_299(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-299",
            "[UBL-CR-299]-[UBL-CR-299]-A UBL invoice should not include the TaxRepresentativeParty LogoReferenceID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cbc:EndpointID)
fn validate_ubl_cr_300(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-300",
            "[UBL-CR-300]-[UBL-CR-300]-A UBL invoice should not include the TaxRepresentativeParty EndpointID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cbc:IndustryClassificationCode)
fn validate_ubl_cr_301(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-301",
            "[UBL-CR-301]-[UBL-CR-301]-A UBL invoice should not include the TaxRepresentativeParty IndustryClassificationCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PartyIdentification)
fn validate_ubl_cr_302(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-302",
            "[UBL-CR-302]-[UBL-CR-302]-A UBL invoice should not include the TaxRepresentativeParty PartyIdentification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:Language)
fn validate_ubl_cr_303(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-303",
            "[UBL-CR-303]-[UBL-CR-303]-A UBL invoice should not include the TaxRepresentativeParty Language",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:ID)
fn validate_ubl_cr_304(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-304",
            "[UBL-CR-304]-[UBL-CR-304]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:AddressTypeCode)
fn validate_ubl_cr_305(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-305",
            "[UBL-CR-305]-[UBL-CR-305]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress AddressTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:AddressFormatCode)
fn validate_ubl_cr_306(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-306",
            "[UBL-CR-306]-[UBL-CR-306]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress AddressFormatCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:Postbox)
fn validate_ubl_cr_307(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-307",
            "[UBL-CR-307]-[UBL-CR-307]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress Postbox",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:Floor)
fn validate_ubl_cr_308(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-308",
            "[UBL-CR-308]-[UBL-CR-308]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress Floor",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:Room)
fn validate_ubl_cr_309(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-309",
            "[UBL-CR-309]-[UBL-CR-309]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress Room",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:BlockName)
fn validate_ubl_cr_310(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-310",
            "[UBL-CR-310]-[UBL-CR-310]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress BlockName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:BuildingName)
fn validate_ubl_cr_311(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-311",
            "[UBL-CR-311]-[UBL-CR-311]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress BuildingName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:BuildingNumber)
fn validate_ubl_cr_312(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-312",
            "[UBL-CR-312]-[UBL-CR-312]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress BuildingNumber",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:InhouseMail)
fn validate_ubl_cr_313(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-313",
            "[UBL-CR-313]-[UBL-CR-313]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress InhouseMail",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:Department)
fn validate_ubl_cr_314(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-314",
            "[UBL-CR-314]-[UBL-CR-314]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress Department",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:MarkAttention)
fn validate_ubl_cr_315(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-315",
            "[UBL-CR-315]-[UBL-CR-315]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress MarkAttention",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:MarkCare)
fn validate_ubl_cr_316(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-316",
            "[UBL-CR-316]-[UBL-CR-316]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress MarkCare",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:PlotIdentification)
fn validate_ubl_cr_317(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-317",
            "[UBL-CR-317]-[UBL-CR-317]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress PlotIdentification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:CitySubdivisionName)
fn validate_ubl_cr_318(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-318",
            "[UBL-CR-318]-[UBL-CR-318]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress CitySubdivisionName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:CountrySubentityCode)
fn validate_ubl_cr_319(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-319",
            "[UBL-CR-319]-[UBL-CR-319]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress CountrySubentityCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:Region)
fn validate_ubl_cr_320(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-320",
            "[UBL-CR-320]-[UBL-CR-320]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress Region",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:District)
fn validate_ubl_cr_321(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-321",
            "[UBL-CR-321]-[UBL-CR-321]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress District",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cbc:TimezoneOffset)
fn validate_ubl_cr_322(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-322",
            "[UBL-CR-322]-[UBL-CR-322]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress TimezoneOffset",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cac:Country/cbc:Name)
fn validate_ubl_cr_323(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-323",
            "[UBL-CR-323]-[UBL-CR-323]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress Country Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PostalAddress/cac:LocationCoordinate)
fn validate_ubl_cr_324(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-324",
            "[UBL-CR-324]-[UBL-CR-324]-A UBL invoice should not include the TaxRepresentativeParty PostalAddress LocationCoordinate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PhysicalLocation)
fn validate_ubl_cr_325(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-325",
            "[UBL-CR-325]-[UBL-CR-325]-A UBL invoice should not include the TaxRepresentativeParty PhysicalLocation",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PartyTaxScheme/cbc:RegistrationName)
fn validate_ubl_cr_326(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-326",
            "[UBL-CR-326]-[UBL-CR-326]-A UBL invoice should not include the TaxRepresentativeParty PartyTaxScheme RegistrationName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PartyTaxScheme/cbc:TaxLevelCode)
fn validate_ubl_cr_327(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-327",
            "[UBL-CR-327]-[UBL-CR-327]-A UBL invoice should not include the TaxRepresentativeParty PartyTaxScheme TaxLevelCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PartyTaxScheme/cbc:ExemptionReasonCode)
fn validate_ubl_cr_328(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-328",
            "[UBL-CR-328]-[UBL-CR-328]-A UBL invoice should not include the TaxRepresentativeParty PartyTaxScheme ExemptionReasonCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PartyTaxScheme/cbc:ExemptionReason)
fn validate_ubl_cr_329(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-329",
            "[UBL-CR-329]-[UBL-CR-329]-A UBL invoice should not include the TaxRepresentativeParty PartyTaxScheme ExemptionReason",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PartyTaxScheme/cac:RegistrationAddress)
fn validate_ubl_cr_330(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-330",
            "[UBL-CR-330]-[UBL-CR-330]-A UBL invoice should not include the TaxRepresentativeParty PartyTaxScheme RegistrationAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PartyTaxScheme/cac:TaxScheme/cbc:Name)
fn validate_ubl_cr_331(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-331",
            "[UBL-CR-331]-[UBL-CR-331]-A UBL invoice should not include the TaxRepresentativeParty PartyTaxScheme TaxScheme Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PartyTaxScheme/cac:TaxScheme/cbc:TaxTypeCode)
fn validate_ubl_cr_332(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-332",
            "[UBL-CR-332]-[UBL-CR-332]-A UBL invoice should not include the TaxRepresentativeParty PartyTaxScheme TaxScheme TaxTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PartyTaxScheme/cac:TaxScheme/cbc:CurrencyCode)
fn validate_ubl_cr_333(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-333",
            "[UBL-CR-333]-[UBL-CR-333]-A UBL invoice should not include the TaxRepresentativeParty PartyTaxScheme TaxScheme CurrencyCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PartyTaxScheme/cac:TaxScheme/cac:JurisdictionRegionAddress)
fn validate_ubl_cr_334(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-334",
            "[UBL-CR-334]-[UBL-CR-334]-A UBL invoice should not include the TaxRepresentativeParty PartyTaxScheme TaxScheme JurisdictionRegionAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PartyLegalEntity)
fn validate_ubl_cr_335(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-335",
            "[UBL-CR-335]-[UBL-CR-335]-A UBL invoice should not include the TaxRepresentativeParty PartyLegalEntity",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:Contact)
fn validate_ubl_cr_336(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-336",
            "[UBL-CR-336]-[UBL-CR-336]-A UBL invoice should not include the TaxRepresentativeParty Contact",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:Person)
fn validate_ubl_cr_337(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-337",
            "[UBL-CR-337]-[UBL-CR-337]-A UBL invoice should not include the TaxRepresentativeParty Person",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:AgentParty)
fn validate_ubl_cr_338(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-338",
            "[UBL-CR-338]-[UBL-CR-338]-A UBL invoice should not include the TaxRepresentativeParty AgentParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:ServiceProviderParty)
fn validate_ubl_cr_339(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-339",
            "[UBL-CR-339]-[UBL-CR-339]-A UBL invoice should not include the TaxRepresentativeParty ServiceProviderParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:PowerOfAttorney)
fn validate_ubl_cr_340(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-340",
            "[UBL-CR-340]-[UBL-CR-340]-A UBL invoice should not include the TaxRepresentativeParty PowerOfAttorney",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxRepresentativeParty/cac:FinancialAccount)
fn validate_ubl_cr_341(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-341",
            "[UBL-CR-341]-[UBL-CR-341]-A UBL invoice should not include the TaxRepresentativeParty FinancialAccount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cbc:ID)
fn validate_ubl_cr_342(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-342",
            "[UBL-CR-342]-[UBL-CR-342]-A UBL invoice should not include the Delivery ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cbc:Quantity)
fn validate_ubl_cr_343(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-343",
            "[UBL-CR-343]-[UBL-CR-343]-A UBL invoice should not include the Delivery Quantity",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cbc:MinimumQuantity)
fn validate_ubl_cr_344(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-344",
            "[UBL-CR-344]-[UBL-CR-344]-A UBL invoice should not include the Delivery MinimumQuantity",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cbc:MaximumQuantity)
fn validate_ubl_cr_345(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-345",
            "[UBL-CR-345]-[UBL-CR-345]-A UBL invoice should not include the Delivery MaximumQuantity",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cbc:ActualDeliveryTime)
fn validate_ubl_cr_346(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-346",
            "[UBL-CR-346]-[UBL-CR-346]-A UBL invoice should not include the Delivery ActualDeliveryTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cbc:LatestDeliveryDate)
fn validate_ubl_cr_347(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-347",
            "[UBL-CR-347]-[UBL-CR-347]-A UBL invoice should not include the Delivery LatestDeliveryDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cbc:LatestDeliveryTime)
fn validate_ubl_cr_348(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-348",
            "[UBL-CR-348]-[UBL-CR-348]-A UBL invoice should not include the Delivery LatestDeliveryTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cbc:ReleaseID)
fn validate_ubl_cr_349(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-349",
            "[UBL-CR-349]-[UBL-CR-349]-A UBL invoice should not include the Delivery ReleaseID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cbc:TrackingID)
fn validate_ubl_cr_350(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-350",
            "[UBL-CR-350]-[UBL-CR-350]-A UBL invoice should not include the Delivery TrackingID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cbc:Description)
fn validate_ubl_cr_351(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-351",
            "[UBL-CR-351]-[UBL-CR-351]-A UBL invoice should not include the Delivery DeliveryLocation Description",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cbc:Conditions)
fn validate_ubl_cr_352(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-352",
            "[UBL-CR-352]-[UBL-CR-352]-A UBL invoice should not include the Delivery DeliveryLocation Conditions",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cbc:CountrySubentity)
fn validate_ubl_cr_353(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-353",
            "[UBL-CR-353]-[UBL-CR-353]-A UBL invoice should not include the Delivery DeliveryLocation CountrySubentity",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cbc:CountrySubentityCode)
fn validate_ubl_cr_354(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-354",
            "[UBL-CR-354]-[UBL-CR-354]-A UBL invoice should not include the Delivery DeliveryLocation CountrySubentityCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cbc:LocationTypeCode)
fn validate_ubl_cr_355(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-355",
            "[UBL-CR-355]-[UBL-CR-355]-A UBL invoice should not include the Delivery DeliveryLocation LocationTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cbc:InformationURI)
fn validate_ubl_cr_356(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-356",
            "[UBL-CR-356]-[UBL-CR-356]-A UBL invoice should not include the Delivery DeliveryLocation InformationURI",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cbc:Name)
fn validate_ubl_cr_357(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-357",
            "[UBL-CR-357]-[UBL-CR-357]-A UBL invoice should not include the Delivery DeliveryLocation Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:ValidityPeriod)
fn validate_ubl_cr_358(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-358",
            "[UBL-CR-358]-[UBL-CR-358]-A UBL invoice should not include the Delivery DeliveryLocation ValidityPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:ID)
fn validate_ubl_cr_359(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-359",
            "[UBL-CR-359]-[UBL-CR-359]-A UBL invoice should not include the Delivery DeliveryLocation Address ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:AddressTypeCode)
fn validate_ubl_cr_360(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-360",
            "[UBL-CR-360]-[UBL-CR-360]-A UBL invoice should not include the Delivery DeliveryLocation Address AddressTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:AddressFormatCode)
fn validate_ubl_cr_361(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-361",
            "[UBL-CR-361]-[UBL-CR-361]-A UBL invoice should not include the Delivery DeliveryLocation Address AddressFormatCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:Postbox)
fn validate_ubl_cr_362(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-362",
            "[UBL-CR-362]-[UBL-CR-362]-A UBL invoice should not include the Delivery DeliveryLocation Address Postbox",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:Floor)
fn validate_ubl_cr_363(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-363",
            "[UBL-CR-363]-[UBL-CR-363]-A UBL invoice should not include the Delivery DeliveryLocation Address Floor",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:Room)
fn validate_ubl_cr_364(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-364",
            "[UBL-CR-364]-[UBL-CR-364]-A UBL invoice should not include the Delivery DeliveryLocation Address Room",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:BlockName)
fn validate_ubl_cr_365(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-365",
            "[UBL-CR-365]-[UBL-CR-365]-A UBL invoice should not include the Delivery DeliveryLocation Address BlockName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:BuildingName)
fn validate_ubl_cr_366(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-366",
            "[UBL-CR-366]-[UBL-CR-366]-A UBL invoice should not include the Delivery DeliveryLocation Address BuildingName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:BuildingNumber)
fn validate_ubl_cr_367(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-367",
            "[UBL-CR-367]-[UBL-CR-367]-A UBL invoice should not include the Delivery DeliveryLocation Address BuildingNumber",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:InhouseMail)
fn validate_ubl_cr_368(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-368",
            "[UBL-CR-368]-[UBL-CR-368]-A UBL invoice should not include the Delivery DeliveryLocation Address InhouseMail",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:Department)
fn validate_ubl_cr_369(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-369",
            "[UBL-CR-369]-[UBL-CR-369]-A UBL invoice should not include the Delivery DeliveryLocation Address Department",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:MarkAttention)
fn validate_ubl_cr_370(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-370",
            "[UBL-CR-370]-[UBL-CR-370]-A UBL invoice should not include the Delivery DeliveryLocation Address MarkAttention",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:MarkCare)
fn validate_ubl_cr_371(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-371",
            "[UBL-CR-371]-[UBL-CR-371]-A UBL invoice should not include the Delivery DeliveryLocation Address MarkCare",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:PlotIdentification)
fn validate_ubl_cr_372(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-372",
            "[UBL-CR-372]-[UBL-CR-372]-A UBL invoice should not include the Delivery DeliveryLocation Address PlotIdentification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:CitySubdivisionName)
fn validate_ubl_cr_373(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-373",
            "[UBL-CR-373]-[UBL-CR-373]-A UBL invoice should not include the Delivery DeliveryLocation Address CitySubdivisionName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:CountrySubentityCode)
fn validate_ubl_cr_374(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-374",
            "[UBL-CR-374]-[UBL-CR-374]-A UBL invoice should not include the Delivery DeliveryLocation Address CountrySubentityCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:Region)
fn validate_ubl_cr_375(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-375",
            "[UBL-CR-375]-[UBL-CR-375]-A UBL invoice should not include the Delivery DeliveryLocation Address Region",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:District)
fn validate_ubl_cr_376(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-376",
            "[UBL-CR-376]-[UBL-CR-376]-A UBL invoice should not include the Delivery DeliveryLocation Address District",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cbc:TimezoneOffset)
fn validate_ubl_cr_377(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-377",
            "[UBL-CR-377]-[UBL-CR-377]-A UBL invoice should not include the Delivery DeliveryLocation Address TimezoneOffset",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cac:Country/cbc:Name)
fn validate_ubl_cr_378(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-378",
            "[UBL-CR-378]-[UBL-CR-378]-A UBL invoice should not include the Delivery DeliveryLocation Address Country Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:Address/cac:LocationCoordinate)
fn validate_ubl_cr_379(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-379",
            "[UBL-CR-379]-[UBL-CR-379]-A UBL invoice should not include the Delivery DeliveryLocation Address LocationCoordinate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:SubsidiaryLocation)
fn validate_ubl_cr_380(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-380",
            "[UBL-CR-380]-[UBL-CR-380]-A UBL invoice should not include the Delivery DeliveryLocation SubsidiaryLocation",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryLocation/cac:LocationCoordinate)
fn validate_ubl_cr_381(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-381",
            "[UBL-CR-381]-[UBL-CR-381]-A UBL invoice should not include the Delivery DeliveryLocation LocationCoordinate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:AlternativeDeliveryLocation)
fn validate_ubl_cr_382(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-382",
            "[UBL-CR-382]-[UBL-CR-382]-A UBL invoice should not include the Delivery AlternativeDeliveryLocation",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:RequestedDeliveryPeriod)
fn validate_ubl_cr_383(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-383",
            "[UBL-CR-383]-[UBL-CR-383]-A UBL invoice should not include the Delivery RequestedDeliveryPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:EstimatedDeliveryPeriod)
fn validate_ubl_cr_384(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-384",
            "[UBL-CR-384]-[UBL-CR-384]-A UBL invoice should not include the Delivery EstimatedDeliveryPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:CarrierParty)
fn validate_ubl_cr_385(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-385",
            "[UBL-CR-385]-[UBL-CR-385]-A UBL invoice should not include the Delivery CarrierParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cbc:MarkCareIndicator)
fn validate_ubl_cr_386(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-386",
            "[UBL-CR-386]-[UBL-CR-386]-A UBL invoice should not include the DeliveryParty MarkCareIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cbc:MarkAttentionIndicator)
fn validate_ubl_cr_387(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-387",
            "[UBL-CR-387]-[UBL-CR-387]-A UBL invoice should not include the DeliveryParty MarkAttentionIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cbc:WebsiteURI)
fn validate_ubl_cr_388(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-388",
            "[UBL-CR-388]-[UBL-CR-388]-A UBL invoice should not include the DeliveryParty WebsiteURI",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cbc:LogoReferenceID)
fn validate_ubl_cr_389(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-389",
            "[UBL-CR-389]-[UBL-CR-389]-A UBL invoice should not include the DeliveryParty LogoReferenceID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cbc:EndpointID)
fn validate_ubl_cr_390(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-390",
            "[UBL-CR-390]-[UBL-CR-390]-A UBL invoice should not include the DeliveryParty EndpointID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cbc:IndustryClassificationCode)
fn validate_ubl_cr_391(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-391",
            "[UBL-CR-391]-[UBL-CR-391]-A UBL invoice should not include the DeliveryParty IndustryClassificationCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cac:PartyIdentification)
fn validate_ubl_cr_392(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-392",
            "[UBL-CR-392]-[UBL-CR-392]-A UBL invoice should not include the DeliveryParty PartyIdentification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cac:Language)
fn validate_ubl_cr_393(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-393",
            "[UBL-CR-393]-[UBL-CR-393]-A UBL invoice should not include the DeliveryParty Language",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cac:PostalAddress)
fn validate_ubl_cr_394(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-394",
            "[UBL-CR-394]-[UBL-CR-394]-A UBL invoice should not include the DeliveryParty PostalAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cac:PhysicalLocation)
fn validate_ubl_cr_395(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-395",
            "[UBL-CR-395]-[UBL-CR-395]-A UBL invoice should not include the DeliveryParty PhysicalLocation",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cac:PartyTaxScheme)
fn validate_ubl_cr_396(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-396",
            "[UBL-CR-396]-[UBL-CR-396]-A UBL invoice should not include the DeliveryParty PartyTaxScheme",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cac:PartyLegalEntity)
fn validate_ubl_cr_397(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-397",
            "[UBL-CR-397]-[UBL-CR-397]-A UBL invoice should not include the DeliveryParty PartyLegalEntity",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cac:Contact)
fn validate_ubl_cr_398(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-398",
            "[UBL-CR-398]-[UBL-CR-398]-A UBL invoice should not include the DeliveryParty Contact",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cac:Person)
fn validate_ubl_cr_399(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-399",
            "[UBL-CR-399]-[UBL-CR-399]-A UBL invoice should not include the DeliveryParty Person",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cac:AgentParty)
fn validate_ubl_cr_400(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-400",
            "[UBL-CR-400]-[UBL-CR-400]-A UBL invoice should not include the DeliveryParty AgentParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cac:ServiceProviderParty)
fn validate_ubl_cr_401(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-401",
            "[UBL-CR-401]-[UBL-CR-401]-A UBL invoice should not include the DeliveryParty ServiceProviderParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cac:PowerOfAttorney)
fn validate_ubl_cr_402(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-402",
            "[UBL-CR-402]-[UBL-CR-402]-A UBL invoice should not include the DeliveryParty PowerOfAttorney",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryParty/cac:FinancialAccount)
fn validate_ubl_cr_403(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-403",
            "[UBL-CR-403]-[UBL-CR-403]-A UBL invoice should not include the DeliveryParty FinancialAccount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:NotifyParty)
fn validate_ubl_cr_404(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-404",
            "[UBL-CR-404]-[UBL-CR-404]-A UBL invoice should not include the Delivery NotifyParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:Despatch)
fn validate_ubl_cr_405(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-405",
            "[UBL-CR-405]-[UBL-CR-405]-A UBL invoice should not include the Delivery Despatch",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryTerms)
fn validate_ubl_cr_406(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-406",
            "[UBL-CR-406]-[UBL-CR-406]-A UBL invoice should not include the Delivery DeliveryTerms",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:MinimumDeliveryUnit)
fn validate_ubl_cr_407(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-407",
            "[UBL-CR-407]-[UBL-CR-407]-A UBL invoice should not include the Delivery MinimumDeliveryUnit",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:MaximumDeliveryUnit)
fn validate_ubl_cr_408(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-408",
            "[UBL-CR-408]-[UBL-CR-408]-A UBL invoice should not include the Delivery MaximumDeliveryUnit",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:Shipment)
fn validate_ubl_cr_409(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-409",
            "[UBL-CR-409]-[UBL-CR-409]-A UBL invoice should not include the Delivery Shipment",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:DeliveryTerms)
fn validate_ubl_cr_410(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-410",
            "[UBL-CR-410]-[UBL-CR-410]-A UBL invoice should not include the DeliveryTerms",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cbc:ID)
fn validate_ubl_cr_411(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-411",
            "[UBL-CR-411]-[UBL-CR-411]-A UBL invoice should not include the PaymentMeans ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cbc:PaymentDueDate) or ../cn:CreditNote
fn validate_ubl_cr_412(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-412",
            "[UBL-CR-412]-[UBL-CR-412]-A UBL invoice should not include the PaymentMeans PaymentDueDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cbc:PaymentChannelCode)
fn validate_ubl_cr_413(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-413",
            "[UBL-CR-413]-[UBL-CR-413]-A UBL invoice should not include the PaymentMeans PaymentChannelCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cbc:InstructionID)
fn validate_ubl_cr_414(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-414",
            "[UBL-CR-414]-[UBL-CR-414]-A UBL invoice should not include the PaymentMeans InstructionID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:CardAccount/cbc:CardTypeCode)
fn validate_ubl_cr_415(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-415",
            "[UBL-CR-415]-[UBL-CR-415]-A UBL invoice should not include the PaymentMeans CardAccount CardTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:CardAccount/cbc:ValidityStartDate)
fn validate_ubl_cr_416(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-416",
            "[UBL-CR-416]-[UBL-CR-416]-A UBL invoice should not include the PaymentMeans CardAccount ValidityStartDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:CardAccount/cbc:ExpiryDate)
fn validate_ubl_cr_417(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-417",
            "[UBL-CR-417]-[UBL-CR-417]-A UBL invoice should not include the PaymentMeans CardAccount ExpiryDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:CardAccount/cbc:IssuerID)
fn validate_ubl_cr_418(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-418",
            "[UBL-CR-418]-[UBL-CR-418]-A UBL invoice should not include the PaymentMeans CardAccount IssuerID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:CardAccount/cbc:IssueNumberID)
fn validate_ubl_cr_419(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-419",
            "[UBL-CR-419]-[UBL-CR-419]-A UBL invoice should not include the PaymentMeans CardAccount IssueNumberID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:CardAccount/cbc:CV2ID)
fn validate_ubl_cr_420(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-420",
            "[UBL-CR-420]-[UBL-CR-420]-A UBL invoice should not include the PaymentMeans CardAccount CV2ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:CardAccount/cbc:CardChipCode)
fn validate_ubl_cr_421(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-421",
            "[UBL-CR-421]-[UBL-CR-421]-A UBL invoice should not include the PaymentMeans CardAccount CardChipCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:CardAccount/cbc:ChipApplicationID)
fn validate_ubl_cr_422(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-422",
            "[UBL-CR-422]-[UBL-CR-422]-A UBL invoice should not include the PaymentMeans CardAccount ChipApplicationID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PayeeFinancialAccount/cbc:AliasName)
fn validate_ubl_cr_424(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-424",
            "[UBL-CR-424]-[UBL-CR-424]-A UBL invoice should not include the PaymentMeans PayeeFinancialAccount AliasName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PayeeFinancialAccount/cbc:AccountTypeCode)
fn validate_ubl_cr_425(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-425",
            "[UBL-CR-425]-[UBL-CR-425]-A UBL invoice should not include the PaymentMeans PayeeFinancialAccount AccountTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PayeeFinancialAccount/cbc:AccountFormatCode)
fn validate_ubl_cr_426(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-426",
            "[UBL-CR-426]-[UBL-CR-426]-A UBL invoice should not include the PaymentMeans PayeeFinancialAccount AccountFormatCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PayeeFinancialAccount/cbc:CurrencyCode)
fn validate_ubl_cr_427(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-427",
            "[UBL-CR-427]-[UBL-CR-427]-A UBL invoice should not include the PaymentMeans PayeeFinancialAccount CurrencyCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PayeeFinancialAccount/cbc:PaymentNote)
fn validate_ubl_cr_428(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-428",
            "[UBL-CR-428]-[UBL-CR-428]-A UBL invoice should not include the PaymentMeans PayeeFinancialAccount PaymentNote",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PayeeFinancialAccount/cac:FinancialInstitutionBranch/cbc:Name)
fn validate_ubl_cr_429(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-429",
            "[UBL-CR-429]-[UBL-CR-429]-A UBL invoice should not include the PaymentMeans PayeeFinancialAccount FinancialInstitutionBranch Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PayeeFinancialAccount/cac:FinancialInstitutionBranch/cac:FinancialInstitution/cbc:Name)
fn validate_ubl_cr_430(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-430",
            "[UBL-CR-430]-[UBL-CR-430]-A UBL invoice should not include the PaymentMeans PayeeFinancialAccount FinancialInstitutionBranch FinancialInstitution Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PayeeFinancialAccount/cac:FinancialInstitutionBranch/cac:FinancialInstitution/cac:Address)
fn validate_ubl_cr_431(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-431",
            "[UBL-CR-431]-[UBL-CR-431]-A UBL invoice should not include the PaymentMeans PayeeFinancialAccount FinancialInstitutionBranch FinancialInstitution Address",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PayeeFinancialAccount/cac:FinancialInstitutionBranch/cac:Address)
fn validate_ubl_cr_432(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-432",
            "[UBL-CR-432]-[UBL-CR-432]-A UBL invoice should not include the PaymentMeans PayeeFinancialAccount FinancialInstitutionBranch Address",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PayeeFinancialAccount/cac:Country)
fn validate_ubl_cr_433(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-433",
            "[UBL-CR-433]-[UBL-CR-433]-A UBL invoice should not include the PaymentMeans PayeeFinancialAccount Country",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:CreditAccount)
fn validate_ubl_cr_434(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-434",
            "[UBL-CR-434]-[UBL-CR-434]-A UBL invoice should not include the PaymentMeans CreditAccount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cbc:MandateTypeCode)
fn validate_ubl_cr_435(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-435",
            "[UBL-CR-435]-[UBL-CR-435]-A UBL invoice should not include the PaymentMeans PaymentMandate MandateTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cbc:MaximumPaymentInstructionsNumeric)
fn validate_ubl_cr_436(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-436",
            "[UBL-CR-436]-[UBL-CR-436]-A UBL invoice should not include the PaymentMeans PaymentMandate MaximumPaymentInstructionsNumeric",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cbc:MaximumPaidAmount)
fn validate_ubl_cr_437(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-437",
            "[UBL-CR-437]-[UBL-CR-437]-A UBL invoice should not include the PaymentMeans PaymentMandate MaximumPaidAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cbc:SignatureID)
fn validate_ubl_cr_438(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-438",
            "[UBL-CR-438]-[UBL-CR-438]-A UBL invoice should not include the PaymentMeans PaymentMandate SignatureID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cac:PayerParty)
fn validate_ubl_cr_439(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-439",
            "[UBL-CR-439]-[UBL-CR-439]-A UBL invoice should not include the PaymentMeans PaymentMandate PayerParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cac:PayerFinancialAccount/cbc:Name)
fn validate_ubl_cr_440(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-440",
            "[UBL-CR-440]-[UBL-CR-440]-A UBL invoice should not include the PaymentMeans PaymentMandate PayerFinancialAccount Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cac:PayerFinancialAccount/cbc:AliasName)
fn validate_ubl_cr_441(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-441",
            "[UBL-CR-441]-[UBL-CR-441]-A UBL invoice should not include the PaymentMeans PaymentMandate PayerFinancialAccount AliasName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cac:PayerFinancialAccount/cbc:AccountTypeCode)
fn validate_ubl_cr_442(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-442",
            "[UBL-CR-442]-[UBL-CR-442]-A UBL invoice should not include the PaymentMeans PaymentMandate PayerFinancialAccount AccountTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cac:PayerFinancialAccount/cbc:AccountFormatCode)
fn validate_ubl_cr_443(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-443",
            "[UBL-CR-443]-[UBL-CR-443]-A UBL invoice should not include the PaymentMeans PaymentMandate PayerFinancialAccount AccountFormatCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cac:PayerFinancialAccount/cbc:CurrencyCode)
fn validate_ubl_cr_444(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-444",
            "[UBL-CR-444]-[UBL-CR-444]-A UBL invoice should not include the PaymentMeans PaymentMandate PayerFinancialAccount CurrencyCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cac:PayerFinancialAccount/cbc:PaymentNote)
fn validate_ubl_cr_445(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-445",
            "[UBL-CR-445]-[UBL-CR-445]-A UBL invoice should not include the PaymentMeans PaymentMandate PayerFinancialAccount PaymentNote",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cac:PayerFinancialAccount/cac:FinancialInstitutionBranch)
fn validate_ubl_cr_446(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-446",
            "[UBL-CR-446]-[UBL-CR-446]-A UBL invoice should not include the PaymentMeans PaymentMandate PayerFinancialAccount FinancialInstitutionBranch",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cac:PayerFinancialAccount/cac:Country)
fn validate_ubl_cr_447(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-447",
            "[UBL-CR-447]-[UBL-CR-447]-A UBL invoice should not include the PaymentMeans PaymentMandate PayerFinancialAccount Country",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cac:ValidityPeriod)
fn validate_ubl_cr_448(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-448",
            "[UBL-CR-448]-[UBL-CR-448]-A UBL invoice should not include the PaymentMeans PaymentMandate ValidityPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cac:PaymentReversalPeriod)
fn validate_ubl_cr_449(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-449",
            "[UBL-CR-449]-[UBL-CR-449]-A UBL invoice should not include the PaymentMeans PaymentMandate PaymentReversalPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PaymentMandate/cac:Clause)
fn validate_ubl_cr_450(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-450",
            "[UBL-CR-450]-[UBL-CR-450]-A UBL invoice should not include the PaymentMeans PaymentMandate Clause",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:TradeFinancing)
fn validate_ubl_cr_451(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-451",
            "[UBL-CR-451]-[UBL-CR-451]-A UBL invoice should not include the PaymentMeans TradeFinancing",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:ID)
fn validate_ubl_cr_452(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-452",
            "[UBL-CR-452]-[UBL-CR-452]-A UBL invoice should not include the PaymentTerms ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:PaymentMeansID)
fn validate_ubl_cr_453(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-453",
            "[UBL-CR-453]-[UBL-CR-453]-A UBL invoice should not include the PaymentTerms PaymentMeansID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:PrepaidPaymentReferenceID)
fn validate_ubl_cr_454(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-454",
            "[UBL-CR-454]-[UBL-CR-454]-A UBL invoice should not include the PaymentTerms PrepaidPaymentReferenceID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:ReferenceEventCode)
fn validate_ubl_cr_455(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-455",
            "[UBL-CR-455]-[UBL-CR-455]-A UBL invoice should not include the PaymentTerms ReferenceEventCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:SettlementDiscountPercent)
fn validate_ubl_cr_456(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-456",
            "[UBL-CR-456]-[UBL-CR-456]-A UBL invoice should not include the PaymentTerms SettlementDiscountPercent",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:PenaltySurchargePercent)
fn validate_ubl_cr_457(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-457",
            "[UBL-CR-457]-[UBL-CR-457]-A UBL invoice should not include the PaymentTerms PenaltySurchargePercent",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:PaymentPercent)
fn validate_ubl_cr_458(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-458",
            "[UBL-CR-458]-[UBL-CR-458]-A UBL invoice should not include the PaymentTerms PaymentPercent",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:Amount)
fn validate_ubl_cr_459(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-459",
            "[UBL-CR-459]-[UBL-CR-459]-A UBL invoice should not include the PaymentTerms Amount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:SettlementDiscountAmount)
fn validate_ubl_cr_460(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-460",
            "[UBL-CR-460]-[UBL-CR-460]-A UBL invoice should not include the PaymentTerms SettlementDiscountAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:PenaltyAmount)
fn validate_ubl_cr_461(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-461",
            "[UBL-CR-461]-[UBL-CR-461]-A UBL invoice should not include the PaymentTerms PenaltyAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:PaymentTermsDetailsURI)
fn validate_ubl_cr_462(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-462",
            "[UBL-CR-462]-[UBL-CR-462]-A UBL invoice should not include the PaymentTerms PaymentTermsDetailsURI",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:PaymentDueDate)
fn validate_ubl_cr_463(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-463",
            "[UBL-CR-463]-[UBL-CR-463]-A UBL invoice should not include the PaymentTerms PaymentDueDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:InstallmentDueDate)
fn validate_ubl_cr_464(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-464",
            "[UBL-CR-464]-[UBL-CR-464]-A UBL invoice should not include the PaymentTerms InstallmentDueDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cbc:InvoicingPartyReference)
fn validate_ubl_cr_465(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-465",
            "[UBL-CR-465]-[UBL-CR-465]-A UBL invoice should not include the PaymentTerms InvoicingPartyReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cac:SettlementPeriod)
fn validate_ubl_cr_466(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-466",
            "[UBL-CR-466]-[UBL-CR-466]-A UBL invoice should not include the PaymentTerms SettlementPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cac:PenaltyPeriod)
fn validate_ubl_cr_467(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-467",
            "[UBL-CR-467]-[UBL-CR-467]-A UBL invoice should not include the PaymentTerms PenaltyPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cac:ExchangeRate)
fn validate_ubl_cr_468(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-468",
            "[UBL-CR-468]-[UBL-CR-468]-A UBL invoice should not include the PaymentTerms ExchangeRate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentTerms/cac:ValidityPeriod)
fn validate_ubl_cr_469(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-469",
            "[UBL-CR-469]-[UBL-CR-469]-A UBL invoice should not include the PaymentTerms ValidityPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PrepaidPayment)
fn validate_ubl_cr_470(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-470",
            "[UBL-CR-470]-[UBL-CR-470]-A UBL invoice should not include the PrepaidPayment",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cbc:ID)
fn validate_ubl_cr_471(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-471",
            "[UBL-CR-471]-[UBL-CR-471]-A UBL invoice should not include the AllowanceCharge ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cbc:PrepaidIndicator)
fn validate_ubl_cr_472(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-472",
            "[UBL-CR-472]-[UBL-CR-472]-A UBL invoice should not include the AllowanceCharge PrepaidIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cbc:SequenceNumeric)
fn validate_ubl_cr_473(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-473",
            "[UBL-CR-473]-[UBL-CR-473]-A UBL invoice should not include the AllowanceCharge SequenceNumeric",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cbc:AccountingCostCode)
fn validate_ubl_cr_474(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-474",
            "[UBL-CR-474]-[UBL-CR-474]-A UBL invoice should not include the AllowanceCharge AccountingCostCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cbc:AccountingCost)
fn validate_ubl_cr_475(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-475",
            "[UBL-CR-475]-[UBL-CR-475]-A UBL invoice should not include the AllowanceCharge AccountingCost",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cbc:PerUnitAmount)
fn validate_ubl_cr_476(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-476",
            "[UBL-CR-476]-[UBL-CR-476]-A UBL invoice should not include the AllowanceCharge PerUnitAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cac:TaxCategory/cbc:Name)
fn validate_ubl_cr_477(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-477",
            "[UBL-CR-477]-[UBL-CR-477]-A UBL invoice should not include the AllowanceCharge TaxCategory Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cac:TaxCategory/cbc:BaseUnitMeasure)
fn validate_ubl_cr_478(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-478",
            "[UBL-CR-478]-[UBL-CR-478]-A UBL invoice should not include the AllowanceCharge TaxCategory BaseUnitMeasure",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cac:TaxCategory/cbc:PerUnitAmount)
fn validate_ubl_cr_479(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-479",
            "[UBL-CR-479]-[UBL-CR-479]-A UBL invoice should not include the AllowanceCharge TaxCategory PerUnitAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cac:TaxCategory/cbc:TaxExemptionReasonCode)
fn validate_ubl_cr_480(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-480",
            "[UBL-CR-480]-[UBL-CR-480]-A UBL invoice should not include the AllowanceCharge TaxCategory TaxExemptionReasonCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cac:TaxCategory/cbc:TaxExemptionReason)
fn validate_ubl_cr_481(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-481",
            "[UBL-CR-481]-[UBL-CR-481]-A UBL invoice should not include the AllowanceCharge TaxCategory TaxExemptionReason",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cac:TaxCategory/cbc:TierRange)
fn validate_ubl_cr_482(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-482",
            "[UBL-CR-482]-[UBL-CR-482]-A UBL invoice should not include the AllowanceCharge TaxCategory TierRange",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cac:TaxCategory/cbc:TierRatePercent)
fn validate_ubl_cr_483(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-483",
            "[UBL-CR-483]-[UBL-CR-483]-A UBL invoice should not include the AllowanceCharge TaxCategory TierRatePercent",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cac:TaxCategory/cac:TaxScheme/cbc:Name)
fn validate_ubl_cr_484(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-484",
            "[UBL-CR-484]-[UBL-CR-484]-A UBL invoice should not include the AllowanceCharge TaxCategory TaxScheme Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cac:TaxCategory/cac:TaxScheme/cbc:TaxTypeCode)
fn validate_ubl_cr_485(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-485",
            "[UBL-CR-485]-[UBL-CR-485]-A UBL invoice should not include the AllowanceCharge TaxCategory TaxScheme TaxTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cac:TaxCategory/cac:TaxScheme/cbc:CurrencyCode)
fn validate_ubl_cr_486(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-486",
            "[UBL-CR-486]-[UBL-CR-486]-A UBL invoice should not include the AllowanceCharge TaxCategory TaxScheme CurrencyCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cac:TaxCategory/cac:TaxScheme/cac:JurisdictionRegionAddress)
fn validate_ubl_cr_487(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-487",
            "[UBL-CR-487]-[UBL-CR-487]-A UBL invoice should not include the AllowanceCharge TaxCategory TaxScheme JurisdictionRegionAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cac:TaxTotal)
fn validate_ubl_cr_488(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-488",
            "[UBL-CR-488]-[UBL-CR-488]-A UBL invoice should not include the AllowanceCharge TaxTotal",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AllowanceCharge/cac:PaymentMeans)
fn validate_ubl_cr_489(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-489",
            "[UBL-CR-489]-[UBL-CR-489]-A UBL invoice should not include the AllowanceCharge PaymentMeans",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxExchangeRate)
fn validate_ubl_cr_490(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-490",
            "[UBL-CR-490]-[UBL-CR-490]-A UBL invoice should not include the TaxExchangeRate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PricingExchangeRate)
fn validate_ubl_cr_491(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-491",
            "[UBL-CR-491]-[UBL-CR-491]-A UBL invoice should not include the PricingExchangeRate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentExchangeRate)
fn validate_ubl_cr_492(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-492",
            "[UBL-CR-492]-[UBL-CR-492]-A UBL invoice should not include the PaymentExchangeRate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentAlternativeExchangeRate)
fn validate_ubl_cr_493(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-493",
            "[UBL-CR-493]-[UBL-CR-493]-A UBL invoice should not include the PaymentAlternativeExchangeRate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cbc:RoundingAmount)
fn validate_ubl_cr_494(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-494",
            "[UBL-CR-494]-[UBL-CR-494]-A UBL invoice should not include the TaxTotal RoundingAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cbc:TaxEvidenceIndicator)
fn validate_ubl_cr_495(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-495",
            "[UBL-CR-495]-[UBL-CR-495]-A UBL invoice should not include the TaxTotal TaxEvidenceIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cbc:TaxIncludedIndicator)
fn validate_ubl_cr_496(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-496",
            "[UBL-CR-496]-[UBL-CR-496]-A UBL invoice should not include the TaxTotal TaxIncludedIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cbc:CalculationSequenceNumeric)
fn validate_ubl_cr_497(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-497",
            "[UBL-CR-497]-[UBL-CR-497]-A UBL invoice should not include the TaxTotal TaxSubtotal CalulationSequenceNumeric",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cbc:TransactionCurrencyTaxAmount)
fn validate_ubl_cr_498(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-498",
            "[UBL-CR-498]-[UBL-CR-498]-A UBL invoice should not include the TaxTotal TaxSubtotal TransactionCurrencyTaxAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cbc:Percent)
fn validate_ubl_cr_499(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-499",
            "[UBL-CR-499]-[UBL-CR-499]-A UBL invoice should not include the TaxTotal TaxSubtotal Percent",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cbc:BaseUnitMeasure)
fn validate_ubl_cr_500(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-500",
            "[UBL-CR-500]-[UBL-CR-500]-A UBL invoice should not include the TaxTotal TaxSubtotal BaseUnitMeasure",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cbc:PerUnitAmount)
fn validate_ubl_cr_501(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-501",
            "[UBL-CR-501]-[UBL-CR-501]-A UBL invoice should not include the TaxTotal TaxSubtotal PerUnitAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cbc:TierRange)
fn validate_ubl_cr_502(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-502",
            "[UBL-CR-502]-[UBL-CR-502]-A UBL invoice should not include the TaxTotal TaxSubtotal TierRange",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cbc:TierRatePercent)
fn validate_ubl_cr_503(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-503",
            "[UBL-CR-503]-[UBL-CR-503]-A UBL invoice should not include the TaxTotal TaxSubtotal TierRatePercent",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory/cbc:Name)
fn validate_ubl_cr_504(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-504",
            "[UBL-CR-504]-[UBL-CR-504]-A UBL invoice should not include the TaxTotal TaxSubtotal TaxCategory Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory/cbc:BaseUnitMeasure)
fn validate_ubl_cr_505(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-505",
            "[UBL-CR-505]-[UBL-CR-505]-A UBL invoice should not include the TaxTotal TaxSubtotal TaxCategory BaseUnitMeasure",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory/cbc:PerUnitAmount)
fn validate_ubl_cr_506(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-506",
            "[UBL-CR-506]-[UBL-CR-506]-A UBL invoice should not include the TaxTotal TaxSubtotal TaxCategory PerUnitAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory/cbc:TierRange)
fn validate_ubl_cr_507(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-507",
            "[UBL-CR-507]-[UBL-CR-507]-A UBL invoice should not include the TaxTotal TaxSubtotal TaxCategory TierRange",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory/cbc:TierRatePercent)
fn validate_ubl_cr_508(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-508",
            "[UBL-CR-508]-[UBL-CR-508]-A UBL invoice should not include the TaxTotal TaxSubtotal TaxCategory TierRatePercent",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory/cac:TaxScheme/cbc:Name)
fn validate_ubl_cr_509(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-509",
            "[UBL-CR-509]-[UBL-CR-509]-A UBL invoice should not include the TaxTotal TaxSubtotal TaxCategory TaxScheme Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory/cac:TaxScheme/cbc:TaxTypeCode)
fn validate_ubl_cr_510(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-510",
            "[UBL-CR-510]-[UBL-CR-510]-A UBL invoice should not include the TaxTotal TaxSubtotal TaxCategory TaxScheme TaxTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory/cac:TaxScheme/cbc:CurrencyCode)
fn validate_ubl_cr_511(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-511",
            "[UBL-CR-511]-[UBL-CR-511]-A UBL invoice should not include the TaxTotal TaxSubtotal TaxCategory TaxScheme CurrencyCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:TaxTotal/cac:TaxSubtotal/cac:TaxCategory/cac:TaxScheme/cac:JurisdictionRegionAddress)
fn validate_ubl_cr_512(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-512",
            "[UBL-CR-512]-[UBL-CR-512]-A UBL invoice should not include the TaxTotal TaxSubtotal TaxCategory TaxScheme JurisdictionRegionAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:WithholdingTaxTotal)
fn validate_ubl_cr_513(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-513",
            "[UBL-CR-513]-[UBL-CR-513]-A UBL invoice should not include the WithholdingTaxTotal",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:LegalMonetaryTotal/cbc:PayableAlternativeAmount)
fn validate_ubl_cr_514(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-514",
            "[UBL-CR-514]-[UBL-CR-514]-A UBL invoice should not include the LegalMonetaryTotal PayableAlternativeAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cbc:UUID)
fn validate_ubl_cr_515(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-515",
            "[UBL-CR-515]-[UBL-CR-515]-A UBL invoice should not include the InvoiceLine UUID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cbc:TaxPointDate)
fn validate_ubl_cr_516(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-516",
            "[UBL-CR-516]-[UBL-CR-516]-A UBL invoice should not include the InvoiceLine TaxPointDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cbc:AccountingCostCode)
fn validate_ubl_cr_517(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-517",
            "[UBL-CR-517]-[UBL-CR-517]-A UBL invoice should not include the InvoiceLine AccountingCostCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cbc:PaymentPurposeCode)
fn validate_ubl_cr_518(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-518",
            "[UBL-CR-518]-[UBL-CR-518]-A UBL invoice should not include the InvoiceLine PaymentPurposeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cbc:FreeOfChargeIndicator)
fn validate_ubl_cr_519(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-519",
            "[UBL-CR-519]-[UBL-CR-519]-A UBL invoice should not include the InvoiceLine FreeOfChargeIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:InvoicePeriod/cbc:StartTime)
fn validate_ubl_cr_520(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-520",
            "[UBL-CR-520]-[UBL-CR-520]-A UBL invoice should not include the InvoiceLine InvoicePeriod StartTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:InvoicePeriod/cbc:EndTime)
fn validate_ubl_cr_521(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-521",
            "[UBL-CR-521]-[UBL-CR-521]-A UBL invoice should not include the InvoiceLine InvoicePeriod EndTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:InvoicePeriod/cbc:DurationMeasure)
fn validate_ubl_cr_522(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-522",
            "[UBL-CR-522]-[UBL-CR-522]-A UBL invoice should not include the InvoiceLine InvoicePeriod DurationMeasure",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:InvoicePeriod/cbc:DescriptionCode)
fn validate_ubl_cr_523(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-523",
            "[UBL-CR-523]-[UBL-CR-523]-A UBL invoice should not include the InvoiceLine InvoicePeriod DescriptionCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:InvoicePeriod/cbc:Description)
fn validate_ubl_cr_524(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-524",
            "[UBL-CR-524]-[UBL-CR-524]-A UBL invoice should not include the InvoiceLine InvoicePeriod Description",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:OrderLineReference/cbc:SalesOrderLineID)
fn validate_ubl_cr_525(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-525",
            "[UBL-CR-525]-[UBL-CR-525]-A UBL invoice should not include the InvoiceLine OrderLineReference SalesOrderLineID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:OrderLineReference/cbc:UUID)
fn validate_ubl_cr_526(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-526",
            "[UBL-CR-526]-[UBL-CR-526]-A UBL invoice should not include the InvoiceLine OrderLineReference UUID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:OrderLineReference/cbc:LineStatusCode)
fn validate_ubl_cr_527(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-527",
            "[UBL-CR-527]-[UBL-CR-527]-A UBL invoice should not include the InvoiceLine OrderLineReference LineStatusCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:OrderLineReference/cac:OrderReference)
fn validate_ubl_cr_528(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-528",
            "[UBL-CR-528]-[UBL-CR-528]-A UBL invoice should not include the InvoiceLine OrderLineReference OrderReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DespatchLineReference)
fn validate_ubl_cr_529(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-529",
            "[UBL-CR-529]-[UBL-CR-529]-A UBL invoice should not include the InvoiceLine DespatchLineReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:ReceiptLineReference)
fn validate_ubl_cr_530(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-530",
            "[UBL-CR-530]-[UBL-CR-530]-A UBL invoice should not include the InvoiceLine ReceiptLineReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:BillingReference)
fn validate_ubl_cr_531(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-531",
            "[UBL-CR-531]-[UBL-CR-531]-A UBL invoice should not include the InvoiceLine BillingReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cbc:CopyIndicator)
fn validate_ubl_cr_532(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-532",
            "[UBL-CR-532]-[UBL-CR-532]-A UBL invoice should not include the InvoiceLine DocumentReference CopyIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cbc:UUID)
fn validate_ubl_cr_533(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-533",
            "[UBL-CR-533]-[UBL-CR-533]-A UBL invoice should not include the InvoiceLine DocumentReference UUID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cbc:IssueDate)
fn validate_ubl_cr_534(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-534",
            "[UBL-CR-534]-[UBL-CR-534]-A UBL invoice should not include the InvoiceLine DocumentReference IssueDate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cbc:IssueTime)
fn validate_ubl_cr_535(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-535",
            "[UBL-CR-535]-[UBL-CR-535]-A UBL invoice should not include the InvoiceLine DocumentReference IssueTime",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cbc:DocumentType)
fn validate_ubl_cr_537(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-537",
            "[UBL-CR-537]-[UBL-CR-537]-A UBL invoice should not include the InvoiceLine DocumentReference DocumentType",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cbc:XPath)
fn validate_ubl_cr_538(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-538",
            "[UBL-CR-538]-[UBL-CR-538]-A UBL invoice should not include the InvoiceLine DocumentReference Xpath",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cbc:LanguageID)
fn validate_ubl_cr_539(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-539",
            "[UBL-CR-539]-[UBL-CR-539]-A UBL invoice should not include the InvoiceLine DocumentReference LanguageID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cbc:LocaleCode)
fn validate_ubl_cr_540(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-540",
            "[UBL-CR-540]-[UBL-CR-540]-A UBL invoice should not include the InvoiceLine DocumentReference LocaleCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cbc:VersionID)
fn validate_ubl_cr_541(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-541",
            "[UBL-CR-541]-[UBL-CR-541]-A UBL invoice should not include the InvoiceLine DocumentReference VersionID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cbc:DocumentStatusCode)
fn validate_ubl_cr_542(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-542",
            "[UBL-CR-542]-[UBL-CR-542]-A UBL invoice should not include the InvoiceLine DocumentReference DocumentStatusCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cbc:DocumentDescription)
fn validate_ubl_cr_543(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-543",
            "[UBL-CR-543]-[UBL-CR-543]-A UBL invoice should not include the InvoiceLine DocumentReference DocumentDescription",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cac:Attachment)
fn validate_ubl_cr_544(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-544",
            "[UBL-CR-544]-[UBL-CR-544]-A UBL invoice should not include the InvoiceLine DocumentReference Attachment",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cac:ValidityPeriod)
fn validate_ubl_cr_545(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-545",
            "[UBL-CR-545]-[UBL-CR-545]-A UBL invoice should not include the InvoiceLine DocumentReference ValidityPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cac:IssuerParty)
fn validate_ubl_cr_546(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-546",
            "[UBL-CR-546]-[UBL-CR-546]-A UBL invoice should not include the InvoiceLine DocumentReference IssuerParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DocumentReference/cac:ResultOfVerification)
fn validate_ubl_cr_547(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-547",
            "[UBL-CR-547]-[UBL-CR-547]-A UBL invoice should not include the InvoiceLine DocumentReference ResultOfVerification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:PricingReference)
fn validate_ubl_cr_548(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-548",
            "[UBL-CR-548]-[UBL-CR-548]-A UBL invoice should not include the InvoiceLine PricingReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:OriginatorParty)
fn validate_ubl_cr_549(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-549",
            "[UBL-CR-549]-[UBL-CR-549]-A UBL invoice should not include the InvoiceLine OriginatorParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Delivery)
fn validate_ubl_cr_550(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-550",
            "[UBL-CR-550]-[UBL-CR-550]-A UBL invoice should not include the InvoiceLine Delivery",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:PaymentTerms)
fn validate_ubl_cr_551(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-551",
            "[UBL-CR-551]-[UBL-CR-551]-A UBL invoice should not include the InvoiceLine PaymentTerms",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:AllowanceCharge/cbc:ID)
fn validate_ubl_cr_552(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-552",
            "[UBL-CR-552]-[UBL-CR-552]-A UBL invoice should not include the InvoiceLine AllowanceCharge ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:AllowanceCharge/cbc:PrepaidIndicator)
fn validate_ubl_cr_553(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-553",
            "[UBL-CR-553]-[UBL-CR-553]-A UBL invoice should not include the InvoiceLine AllowanceCharge PrepaidIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:AllowanceCharge/cbc:SequenceNumeric)
fn validate_ubl_cr_554(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-554",
            "[UBL-CR-554]-[UBL-CR-554]-A UBL invoice should not include the InvoiceLine AllowanceCharge SequenceNumeric",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:AllowanceCharge/cbc:AccountingCostCode)
fn validate_ubl_cr_555(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-555",
            "[UBL-CR-555]-[UBL-CR-555]-A UBL invoice should not include the InvoiceLine AllowanceCharge AccountingCostCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:AllowanceCharge/cbc:AccountingCost)
fn validate_ubl_cr_556(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-556",
            "[UBL-CR-556]-[UBL-CR-556]-A UBL invoice should not include the InvoiceLine AllowanceCharge AccountingCost",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:AllowanceCharge/cbc:PerUnitAmount)
fn validate_ubl_cr_557(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-557",
            "[UBL-CR-557]-[UBL-CR-557]-A UBL invoice should not include the InvoiceLine AllowanceCharge PerUnitAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:AllowanceCharge/cac:TaxCategory)
fn validate_ubl_cr_558(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-558",
            "[UBL-CR-558]-[UBL-CR-558]-A UBL invoice should not include the InvoiceLine AllowanceCharge TaxCategory",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:AllowanceCharge/cac:TaxTotal)
fn validate_ubl_cr_559(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-559",
            "[UBL-CR-559]-[UBL-CR-559]-A UBL invoice should not include the InvoiceLine AllowanceCharge TaxTotal",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:AllowanceCharge/cac:PaymentMeans)
fn validate_ubl_cr_560(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-560",
            "[UBL-CR-560]-[UBL-CR-560]-A UBL invoice should not include the InvoiceLine AllowanceCharge PaymentMeans",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:TaxTotal)
fn validate_ubl_cr_561(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-561",
            "[UBL-CR-561]-[UBL-CR-561]-A UBL invoice should not include the InvoiceLine TaxTotal",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:WithholdingTaxTotal)
fn validate_ubl_cr_562(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-562",
            "[UBL-CR-562]-[UBL-CR-562]-A UBL invoice should not include the InvoiceLine WithholdingTaxTotal",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cbc:PackQuantity)
fn validate_ubl_cr_563(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-563",
            "[UBL-CR-563]-[UBL-CR-563]-A UBL invoice should not include the InvoiceLine Item PackQuantity",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cbc:PackSizeNumeric)
fn validate_ubl_cr_564(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-564",
            "[UBL-CR-564]-[UBL-CR-564]-A UBL invoice should not include the InvoiceLine Item PackSizeNumeric",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cbc:CatalogueIndicator)
fn validate_ubl_cr_565(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-565",
            "[UBL-CR-565]-[UBL-CR-565]-A UBL invoice should not include the InvoiceLine Item CatalogueIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cbc:HazardousRiskIndicator)
fn validate_ubl_cr_566(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-566",
            "[UBL-CR-566]-[UBL-CR-566]-A UBL invoice should not include the InvoiceLine Item HazardousRiskIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cbc:AdditionalInformation)
fn validate_ubl_cr_567(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-567",
            "[UBL-CR-567]-[UBL-CR-567]-A UBL invoice should not include the InvoiceLine Item AdditionalInformation",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cbc:Keyword)
fn validate_ubl_cr_568(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-568",
            "[UBL-CR-568]-[UBL-CR-568]-A UBL invoice should not include the InvoiceLine Item Keyword",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cbc:BrandName)
fn validate_ubl_cr_569(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-569",
            "[UBL-CR-569]-[UBL-CR-569]-A UBL invoice should not include the InvoiceLine Item BrandName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cbc:ModelName)
fn validate_ubl_cr_570(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-570",
            "[UBL-CR-570]-[UBL-CR-570]-A UBL invoice should not include the InvoiceLine Item ModelName",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:BuyersItemIdentification/cbc:ExtendedID)
fn validate_ubl_cr_571(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-571",
            "[UBL-CR-571]-[UBL-CR-571]-A UBL invoice should not include the InvoiceLine Item BuyersItemIdentification ExtendedID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:BuyersItemIdentification/cbc:BarcodeSymbologyID)
fn validate_ubl_cr_572(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-572",
            "[UBL-CR-572]-[UBL-CR-572]-A UBL invoice should not include the InvoiceLine Item BuyersItemIdentification BarecodeSymbologyID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:BuyersItemIdentification/cac:PhysicalAttribute)
fn validate_ubl_cr_573(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-573",
            "[UBL-CR-573]-[UBL-CR-573]-A UBL invoice should not include the InvoiceLine Item BuyersItemIdentification PhysicalAttribute",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:BuyersItemIdentification/cac:MeasurementDimension)
fn validate_ubl_cr_574(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-574",
            "[UBL-CR-574]-[UBL-CR-574]-A UBL invoice should not include the InvoiceLine Item BuyersItemIdentification MeasurementDimension",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:BuyersItemIdentification/cac:IssuerParty)
fn validate_ubl_cr_575(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-575",
            "[UBL-CR-575]-[UBL-CR-575]-A UBL invoice should not include the InvoiceLine Item BuyersItemIdentification IssuerParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:SellersItemIdentification/cbc:ExtendedID)
fn validate_ubl_cr_576(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-576",
            "[UBL-CR-576]-[UBL-CR-576]-A UBL invoice should not include the InvoiceLine Item SellersItemIdentification ExtendedID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:SellersItemIdentification/cbc:BarcodeSymbologyID)
fn validate_ubl_cr_577(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-577",
            "[UBL-CR-577]-[UBL-CR-577]-A UBL invoice should not include the InvoiceLine Item SellersItemIdentification BarecodeSymbologyID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:SellersItemIdentification/cac:PhysicalAttribute)
fn validate_ubl_cr_578(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-578",
            "[UBL-CR-578]-[UBL-CR-578]-A UBL invoice should not include the InvoiceLine Item SellersItemIdentification PhysicalAttribute",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:SellersItemIdentification/cac:MeasurementDimension)
fn validate_ubl_cr_579(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-579",
            "[UBL-CR-579]-[UBL-CR-579]-A UBL invoice should not include the InvoiceLine Item SellersItemIdentification MeasurementDimension",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:SellersItemIdentification/cac:IssuerParty)
fn validate_ubl_cr_580(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-580",
            "[UBL-CR-580]-[UBL-CR-580]-A UBL invoice should not include the InvoiceLine Item SellersItemIdentification IssuerParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ManufacturersItemIdentification)
fn validate_ubl_cr_581(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-581",
            "[UBL-CR-581]-[UBL-CR-581]-A UBL invoice should not include the InvoiceLine Item ManufacturersItemIdentification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:StandardItemIdentification/cbc:ExtendedID)
fn validate_ubl_cr_582(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-582",
            "[UBL-CR-582]-[UBL-CR-582]-A UBL invoice should not include the InvoiceLine Item StandardItemIdentification ExtendedID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:StandardItemIdentification/cbc:BarcodeSymbologyID)
fn validate_ubl_cr_583(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-583",
            "[UBL-CR-583]-[UBL-CR-583]-A UBL invoice should not include the InvoiceLine Item StandardItemIdentification BarecodeSymbologyID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:StandardItemIdentification/cac:PhysicalAttribute)
fn validate_ubl_cr_584(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-584",
            "[UBL-CR-584]-[UBL-CR-584]-A UBL invoice should not include the InvoiceLine Item StandardItemIdentification PhysicalAttribute",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:StandardItemIdentification/cac:MeasurementDimension)
fn validate_ubl_cr_585(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-585",
            "[UBL-CR-585]-[UBL-CR-585]-A UBL invoice should not include the InvoiceLine Item StandardItemIdentification MeasurementDimension",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:StandardItemIdentification/cac:IssuerParty)
fn validate_ubl_cr_586(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-586",
            "[UBL-CR-586]-[UBL-CR-586]-A UBL invoice should not include the InvoiceLine Item StandardItemIdentification IssuerParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:CatalogueItemIdentification)
fn validate_ubl_cr_587(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-587",
            "[UBL-CR-587]-[UBL-CR-587]-A UBL invoice should not include the InvoiceLine Item CatalogueItemIdentification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:AdditionalItemIdentification)
fn validate_ubl_cr_588(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-588",
            "[UBL-CR-588]-[UBL-CR-588]-A UBL invoice should not include the InvoiceLine Item AdditionalItemIdentification",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:CatalogueDocumentReference)
fn validate_ubl_cr_589(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-589",
            "[UBL-CR-589]-[UBL-CR-589]-A UBL invoice should not include the InvoiceLine Item CatalogueDocumentReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ItemSpecificationDocumentReference)
fn validate_ubl_cr_590(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-590",
            "[UBL-CR-590]-[UBL-CR-590]-A UBL invoice should not include the InvoiceLine Item ItemSpecificationDocumentReference",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:OriginCountry/cbc:Name)
fn validate_ubl_cr_591(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-591",
            "[UBL-CR-591]-[UBL-CR-591]-A UBL invoice should not include the InvoiceLine Item OriginCountry Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:CommodityClassification/cbc:NatureCode)
fn validate_ubl_cr_592(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-592",
            "[UBL-CR-592]-[UBL-CR-592]-A UBL invoice should not include the InvoiceLine Item CommodityClassification NatureCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:CommodityClassification/cbc:CargoTypeCode)
fn validate_ubl_cr_593(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-593",
            "[UBL-CR-593]-[UBL-CR-593]-A UBL invoice should not include the InvoiceLine Item CommodityClassification CargoTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:CommodityClassification/cbc:CommodityCode)
fn validate_ubl_cr_594(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-594",
            "[UBL-CR-594]-[UBL-CR-594]-A UBL invoice should not include the InvoiceLine Item CommodityClassification CommodityCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:TransactionConditions)
fn validate_ubl_cr_595(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-595",
            "[UBL-CR-595]-[UBL-CR-595]-A UBL invoice should not include the InvoiceLine Item TransactionConditions",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:HazardousItem)
fn validate_ubl_cr_596(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-596",
            "[UBL-CR-596]-[UBL-CR-596]-A UBL invoice should not include the InvoiceLine Item HazardousItem",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ClassifiedTaxCategory/cbc:Name)
fn validate_ubl_cr_597(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-597",
            "[UBL-CR-597]-[UBL-CR-597]-A UBL invoice should not include the InvoiceLine Item ClassifiedTaxCategory Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ClassifiedTaxCategory/cbc:BaseUnitMeasure)
fn validate_ubl_cr_598(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-598",
            "[UBL-CR-598]-[UBL-CR-598]-A UBL invoice should not include the InvoiceLine Item ClassifiedTaxCategory BaseUnitMeasure",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ClassifiedTaxCategory/cbc:PerUnitAmount)
fn validate_ubl_cr_599(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-599",
            "[UBL-CR-599]-[UBL-CR-599]-A UBL invoice should not include the InvoiceLine Item ClassifiedTaxCategory PerUnitAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ClassifiedTaxCategory/cbc:TaxExemptionReasonCode)
fn validate_ubl_cr_600(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-600",
            "[UBL-CR-600]-[UBL-CR-600]-A UBL invoice should not include the InvoiceLine Item ClassifiedTaxCategory TaxExemptionReasonCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ClassifiedTaxCategory/cbc:TaxExemptionReason)
fn validate_ubl_cr_601(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-601",
            "[UBL-CR-601]-[UBL-CR-601]-A UBL invoice should not include the InvoiceLine Item ClassifiedTaxCategory TaxExemptionReason",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ClassifiedTaxCategory/cbc:TierRange)
fn validate_ubl_cr_602(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-602",
            "[UBL-CR-602]-[UBL-CR-602]-A UBL invoice should not include the InvoiceLine Item ClassifiedTaxCategory TierRange",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ClassifiedTaxCategory/cbc:TierRatePercent)
fn validate_ubl_cr_603(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-603",
            "[UBL-CR-603]-[UBL-CR-603]-A UBL invoice should not include the InvoiceLine Item ClassifiedTaxCategory TierRatePercent",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ClassifiedTaxCategory/cac:TaxScheme/cbc:Name)
fn validate_ubl_cr_604(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-604",
            "[UBL-CR-604]-[UBL-CR-604]-A UBL invoice should not include the InvoiceLine Item ClassifiedTaxCategory TaxScheme Name",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ClassifiedTaxCategory/cac:TaxScheme/cbc:TaxTypeCode)
fn validate_ubl_cr_605(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-605",
            "[UBL-CR-605]-[UBL-CR-605]-A UBL invoice should not include the InvoiceLine Item ClassifiedTaxCategory TaxScheme TaxTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ClassifiedTaxCategory/cac:TaxScheme/cbc:CurrencyCode)
fn validate_ubl_cr_606(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-606",
            "[UBL-CR-606]-[UBL-CR-606]-A UBL invoice should not include the InvoiceLine Item ClassifiedTaxCategory TaxScheme CurrencyCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ClassifiedTaxCategory/cac:TaxScheme/cac:JurisdictionRegionAddress)
fn validate_ubl_cr_607(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-607",
            "[UBL-CR-607]-[UBL-CR-607]-A UBL invoice should not include the InvoiceLine Item ClassifiedTaxCategory TaxScheme JurisdictionRegionAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:AdditionalItemProperty/cbc:ID)
fn validate_ubl_cr_608(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-608",
            "[UBL-CR-608]-[UBL-CR-608]-A UBL invoice should not include the InvoiceLine Item AdditionalItemProperty ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:AdditionalItemProperty/cbc:NameCode)
fn validate_ubl_cr_609(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-609",
            "[UBL-CR-609]-[UBL-CR-609]-A UBL invoice should not include the InvoiceLine Item AdditionalItemProperty NameCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:AdditionalItemProperty/cbc:TestMethod)
fn validate_ubl_cr_610(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-610",
            "[UBL-CR-610]-[UBL-CR-610]-A UBL invoice should not include the InvoiceLine Item AdditionalItemProperty TestMethod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:AdditionalItemProperty/cbc:ValueQuantity)
fn validate_ubl_cr_611(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-611",
            "[UBL-CR-611]-[UBL-CR-611]-A UBL invoice should not include the InvoiceLine Item AdditionalItemProperty ValueQuantity",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:AdditionalItemProperty/cbc:ValueQualifier)
fn validate_ubl_cr_612(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-612",
            "[UBL-CR-612]-[UBL-CR-612]-A UBL invoice should not include the InvoiceLine Item AdditionalItemProperty ValueQualifier",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:AdditionalItemProperty/cbc:ImportanceCode)
fn validate_ubl_cr_613(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-613",
            "[UBL-CR-613]-[UBL-CR-613]-A UBL invoice should not include the InvoiceLine Item AdditionalItemProperty ImportanceCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:AdditionalItemProperty/cbc:ListValue)
fn validate_ubl_cr_614(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-614",
            "[UBL-CR-614]-[UBL-CR-614]-A UBL invoice should not include the InvoiceLine Item AdditionalItemProperty ListValue",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:AdditionalItemProperty/cac:UsabilityPeriod)
fn validate_ubl_cr_615(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-615",
            "[UBL-CR-615]-[UBL-CR-615]-A UBL invoice should not include the InvoiceLine Item AdditionalItemProperty UsabilityPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:AdditionalItemProperty/cac:ItemPropertyGroup)
fn validate_ubl_cr_616(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-616",
            "[UBL-CR-616]-[UBL-CR-616]-A UBL invoice should not include the InvoiceLine Item AdditionalItemProperty ItemPropertyGroup",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:AdditionalItemProperty/cac:RangeDimension)
fn validate_ubl_cr_617(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-617",
            "[UBL-CR-617]-[UBL-CR-617]-A UBL invoice should not include the InvoiceLine Item AdditionalItemProperty RangeDimension",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:AdditionalItemProperty/cac:ItemPropertyRange)
fn validate_ubl_cr_618(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-618",
            "[UBL-CR-618]-[UBL-CR-618]-A UBL invoice should not include the InvoiceLine Item AdditionalItemProperty ItemPropertyRange",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ManufacturerParty)
fn validate_ubl_cr_619(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-619",
            "[UBL-CR-619]-[UBL-CR-619]-A UBL invoice should not include the InvoiceLine Item ManufacturerParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:InformationContentProviderParty)
fn validate_ubl_cr_620(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-620",
            "[UBL-CR-620]-[UBL-CR-620]-A UBL invoice should not include the InvoiceLine Item InformationContentProviderParty",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:OriginAddress)
fn validate_ubl_cr_621(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-621",
            "[UBL-CR-621]-[UBL-CR-621]-A UBL invoice should not include the InvoiceLine Item OriginAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:ItemInstance)
fn validate_ubl_cr_622(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-622",
            "[UBL-CR-622]-[UBL-CR-622]-A UBL invoice should not include the InvoiceLine Item ItemInstance",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:Certificate)
fn validate_ubl_cr_623(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-623",
            "[UBL-CR-623]-[UBL-CR-623]-A UBL invoice should not include the InvoiceLine Item Certificate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Item/cac:Dimension)
fn validate_ubl_cr_624(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-624",
            "[UBL-CR-624]-[UBL-CR-624]-A UBL invoice should not include the InvoiceLine Item Dimension",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cbc:PriceChangeReason)
fn validate_ubl_cr_625(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-625",
            "[UBL-CR-625]-[UBL-CR-625]-A UBL invoice should not include the InvoiceLine Item Price PriceChangeReason",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cbc:PriceTypeCode)
fn validate_ubl_cr_626(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-626",
            "[UBL-CR-626]-[UBL-CR-626]-A UBL invoice should not include the InvoiceLine Item Price PriceTypeCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cbc:PriceType)
fn validate_ubl_cr_627(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-627",
            "[UBL-CR-627]-[UBL-CR-627]-A UBL invoice should not include the InvoiceLine Item Price PriceType",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cbc:OrderableUnitFactorRate)
fn validate_ubl_cr_628(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-628",
            "[UBL-CR-628]-[UBL-CR-628]-A UBL invoice should not include the InvoiceLine Item Price OrderableUnitFactorRate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cbc:ValidityPeriod)
fn validate_ubl_cr_629(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-629",
            "[UBL-CR-629]-[UBL-CR-629]-A UBL invoice should not include the InvoiceLine Item Price ValidityPeriod",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cbc:PriceList)
fn validate_ubl_cr_630(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-630",
            "[UBL-CR-630]-[UBL-CR-630]-A UBL invoice should not include the InvoiceLine Item Price PriceList",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cbc:OrderableUnitFactorRate)
fn validate_ubl_cr_631(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-631",
            "[UBL-CR-631]-[UBL-CR-631]-A UBL invoice should not include the InvoiceLine Item Price OrderableUnitFactorRate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cac:AllowanceCharge/cbc:ID)
fn validate_ubl_cr_632(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-632",
            "[UBL-CR-632]-[UBL-CR-632]-A UBL invoice should not include the InvoiceLine Item Price AllowanceCharge ID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cac:AllowanceCharge/cbc:AllowanceChargeReasonCode)
fn validate_ubl_cr_633(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-633",
            "[UBL-CR-633]-[UBL-CR-633]-A UBL invoice should not include the InvoiceLine Item Price AllowanceCharge AllowanceChargeReasonCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cac:AllowanceCharge/cbc:AllowanceChargeReason)
fn validate_ubl_cr_634(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-634",
            "[UBL-CR-634]-[UBL-CR-634]-A UBL invoice should not include the InvoiceLine Item Price AllowanceCharge AllowanceChargeReason",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cac:AllowanceCharge/cbc:MultiplierFactorNumeric)
fn validate_ubl_cr_635(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-635",
            "[UBL-CR-635]-[UBL-CR-635]-A UBL invoice should not include the InvoiceLine Item Price AllowanceCharge MultiplierFactorNumeric",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cac:AllowanceCharge/cbc:PrepaidIndicator)
fn validate_ubl_cr_636(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-636",
            "[UBL-CR-636]-[UBL-CR-636]-A UBL invoice should not include the InvoiceLine Item Price AllowanceCharge PrepaidIndicator",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cac:AllowanceCharge/cbc:SequenceNumeric)
fn validate_ubl_cr_637(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-637",
            "[UBL-CR-637]-[UBL-CR-637]-A UBL invoice should not include the InvoiceLine Item Price AllowanceCharge SequenceNumeric",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cac:AllowanceCharge/cbc:AccountingCostCode)
fn validate_ubl_cr_638(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-638",
            "[UBL-CR-638]-[UBL-CR-638]-A UBL invoice should not include the InvoiceLine Item Price AllowanceCharge AccountingCostCode",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cac:AllowanceCharge/cbc:AccountingCost)
fn validate_ubl_cr_639(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-639",
            "[UBL-CR-639]-[UBL-CR-639]-A UBL invoice should not include the InvoiceLine Item Price AllowanceCharge AccountingCost",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cac:AllowanceCharge/cbc:PerUnitAmount)
fn validate_ubl_cr_640(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-640",
            "[UBL-CR-640]-[UBL-CR-640]-A UBL invoice should not include the InvoiceLine Item Price AllowanceCharge PerUnitAmount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cac:AllowanceCharge/cac:TaxCategory)
fn validate_ubl_cr_641(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-641",
            "[UBL-CR-641]-[UBL-CR-641]-A UBL invoice should not include the InvoiceLine Item Price AllowanceCharge TaxCategory",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cac:AllowanceCharge/cac:TaxTotal)
fn validate_ubl_cr_642(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-642",
            "[UBL-CR-642]-[UBL-CR-642]-A UBL invoice should not include the InvoiceLine Item Price AllowanceCharge TaxTotal",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cac:AllowanceCharge/cac:PaymentMeans)
fn validate_ubl_cr_643(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-643",
            "[UBL-CR-643]-[UBL-CR-643]-A UBL invoice should not include the InvoiceLine Item Price AllowanceCharge PaymentMeans",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:Price/cac:PricingExchangeRate)
fn validate_ubl_cr_644(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-644",
            "[UBL-CR-644]-[UBL-CR-644]-A UBL invoice should not include the InvoiceLine Item Price PricingExchangeRate",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:DeliveryTerms)
fn validate_ubl_cr_645(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-645",
            "[UBL-CR-645]-[UBL-CR-645]-A UBL invoice should not include the InvoiceLine DeliveryTerms",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:SubInvoiceLine)
fn validate_ubl_cr_646(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-646",
            "[UBL-CR-646]-[UBL-CR-646]-A UBL invoice should not include the InvoiceLine SubInvoiceLine",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not((cac:InvoiceLine|cac:CreditNoteLine)/cac:ItemPriceExtension)
fn validate_ubl_cr_647(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-647",
            "[UBL-CR-647]-[UBL-CR-647]-A UBL invoice should not include the InvoiceLine ItemPriceExtension",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:CustomizationID/@schemeID)
fn validate_ubl_cr_648(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-648",
            "[UBL-CR-648]-[UBL-CR-648]-A UBL invoice should not include the CustomizationID scheme identifier",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:ProfileID/@schemeID)
fn validate_ubl_cr_649(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-649",
            "[UBL-CR-649]-[UBL-CR-649]-A UBL invoice should not include the ProfileID scheme identifier",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:ID/@schemeID)
fn validate_ubl_cr_650(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-650",
            "[UBL-CR-650]-[UBL-CR-650]-A UBL invoice shall not include the Invoice ID scheme identifier",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:SalesOrderID/@schemeID)
fn validate_ubl_cr_651(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-651",
            "[UBL-CR-651]-[UBL-CR-651]-A UBL invoice should not include the SalesOrderID scheme identifier",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:PartyTaxScheme/cbc:CompanyID/@schemeID)
fn validate_ubl_cr_652(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-652",
            "[UBL-CR-652]-[UBL-CR-652]-A UBL invoice should not include the PartyTaxScheme CompanyID scheme identifier",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cbc:PaymentID/@schemeID)
fn validate_ubl_cr_653(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-653",
            "[UBL-CR-653]-[UBL-CR-653]-A UBL invoice should not include the PaymentID scheme identifier",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PayeeFinancialAccount/cbc:ID/@schemeID)
fn validate_ubl_cr_654(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-654",
            "[UBL-CR-654]-[UBL-CR-654]-A UBL invoice should not include the PayeeFinancialAccount scheme identifier",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cac:PayeeFinancialAccount/cac:FinancialInstitutionBranch/cbc:ID/@schemeID)
fn validate_ubl_cr_655(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-655",
            "[UBL-CR-655]-[UBL-CR-655]-A UBL invoice shall not include the FinancialInstitutionBranch ID scheme identifier",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:InvoiceTypeCode/@listID)
fn validate_ubl_cr_656(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-656",
            "[UBL-CR-656]-[UBL-CR-656]-A UBL invoice should not include the InvoiceTypeCode listID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:DocumentCurrencyCode/@listID)
fn validate_ubl_cr_657(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-657",
            "[UBL-CR-657]-[UBL-CR-657]-A UBL invoice should not include the DocumentCurrencyCode listID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:TaxCurrencyCode/@listID)
fn validate_ubl_cr_658(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-658",
            "[UBL-CR-658]-[UBL-CR-658]-A UBL invoice should not include the TaxCurrencyCode listID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:AdditionalDocumentReference/cbc:DocumentTypeCode/@listID)
fn validate_ubl_cr_659(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-659",
            "[UBL-CR-659]-[UBL-CR-659]-A UBL invoice shall not include the AdditionalDocumentReference DocumentTypeCode listID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:Country/cbc:IdentificationCode/@listID)
fn validate_ubl_cr_660(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-660",
            "[UBL-CR-660]-[UBL-CR-660]-A UBL invoice should not include the Country Identification code listID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cbc:PaymentMeansCode/@listID)
fn validate_ubl_cr_661(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-661",
            "[UBL-CR-661]-[UBL-CR-661]-A UBL invoice should not include the PaymentMeansCode listID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cbc:AllowanceChargeReasonCode/@listID)
fn validate_ubl_cr_662(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-662",
            "[UBL-CR-662]-[UBL-CR-662]-A UBL invoice should not include the AllowanceChargeReasonCode listID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@unitCodeListID)
fn validate_ubl_cr_663(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-663",
            "[UBL-CR-663]-[UBL-CR-663]-A UBL invoice should not include the unitCodeListID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:FinancialInstitution)
fn validate_ubl_cr_664(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-664",
            "[UBL-CR-664]-[UBL-CR-664]-A UBL invoice should not include the FinancialInstitutionBranch FinancialInstitution",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:AdditionalDocumentReference[cbc:DocumentTypeCode != '130' or not(cbc:DocumentTypeCode)]/cbc:ID/@schemeID)
fn validate_ubl_cr_665(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-665",
            "[UBL-CR-665]-[UBL-CR-665]-A UBL invoice should not include the AdditionalDocumentReference ID schemeID unless the DocumentTypeCode equals '130'",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:AdditionalDocumentReference[cbc:DocumentTypeCode = '130']/cac:Attachment)
fn validate_ubl_cr_666(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-CR-666",
            "[UBL-CR-666]-[UBL-CR-666]-A UBL invoice shall not include an AdditionalDocumentReference simultaneously referring an Invoice Object Identifier and an Attachment",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:BuyersItemIdentification/cbc:ID/@schemeID)
fn validate_ubl_cr_667(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-667",
            "[UBL-CR-667]-[UBL-CR-667]-A UBL invoice should not include a Buyer Item Identification schemeID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:SellersItemIdentification/cbc:ID/@schemeID)
fn validate_ubl_cr_668(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-668",
            "[UBL-CR-668]-[UBL-CR-668]-A UBL invoice should not include a Sellers Item Identification schemeID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:Price/cac:AllowanceCharge/cbc:AllowanceChargeReasonCode)
fn validate_ubl_cr_669(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-669",
            "[UBL-CR-669]-[UBL-CR-669]-A UBL invoice should not include a Price Allowance Reason Code",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:Price/cac:AllowanceCharge/cbc:AllowanceChargeReason)
fn validate_ubl_cr_670(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-670",
            "[UBL-CR-670]-[UBL-CR-670]-A UBL invoice should not include a Price Allowance Reason",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:Price/cac:AllowanceCharge/cbc:MultiplierFactorNumeric)
fn validate_ubl_cr_671(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-671",
            "[UBL-CR-671]-[UBL-CR-671]-A UBL invoice should not include a Price Allowance Multiplier Factor",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cbc:CreditNoteTypeCode/@listID)
fn validate_ubl_cr_672(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-672",
            "[UBL-CR-672]-[UBL-CR-672]-A UBL credit note should not include the CreditNoteTypeCode listID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:AdditionalDocumentReference[cbc:DocumentTypeCode  = '130']/cbc:DocumentDescription)
fn validate_ubl_cr_673(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-CR-673",
            "[UBL-CR-673]-[UBL-CR-673]-A UBL invoice shall not include an AdditionalDocumentReference simultaneously referring an Invoice Object Identifier and an Document Description",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cbc:PrimaryAccountNumber/@schemeID)
fn validate_ubl_cr_674(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-674",
            "[UBL-CR-674]-[UBL-CR-674]-A UBL invoice should not include the PrimaryAccountNumber schemeID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:CardAccount/cbc:NetworkID/@schemeID)
fn validate_ubl_cr_675(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-675",
            "[UBL-CR-675]-[UBL-CR-675]-A UBL invoice should not include the NetworkID schemeID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:PaymentMandate/cbc:ID/@schemeID)
fn validate_ubl_cr_676(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-676",
            "[UBL-CR-676]-[UBL-CR-676]-A UBL invoice should not include the PaymentMandate/ID schemeID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:PaymentMandate/cac:PayerFinancialAccount/cbc:ID/@schemeID)
fn validate_ubl_cr_677(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-677",
            "[UBL-CR-677]-[UBL-CR-677]-A UBL invoice should not include the PayerFinancialAccount/ID schemeID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:TaxCategory/cbc:ID/@schemeID)
fn validate_ubl_cr_678(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-678",
            "[UBL-CR-678]-[UBL-CR-678]-A UBL invoice should not include the TaxCategory/ID schemeID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:ClassifiedTaxCategory/cbc:ID/@schemeID)
fn validate_ubl_cr_679(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-679",
            "[UBL-CR-679]-[UBL-CR-679]-A UBL invoice should not include the ClassifiedTaxCategory/ID schemeID",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//cac:PaymentMeans/cac:PayerFinancialAccount)
fn validate_ubl_cr_680(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-680",
            "[UBL-CR-680]-[UBL-CR-680]-A UBL invoice should not include the PaymentMeans/PayerFinancialAccount",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:PaymentMeans/cbc:InstructionNote)
fn validate_ubl_cr_681(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-681",
            "[UBL-CR-681]-[UBL-CR-681]-A UBL invoice should not include the PaymentMeans InstructionNote",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(cac:Delivery/cac:DeliveryAddress)
fn validate_ubl_cr_682(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-CR-682",
            "[UBL-CR-682]-[UBL-CR-682]-A UBL invoice should not include the Delivery DeliveryAddress",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@schemeName)
fn validate_ubl_dt_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-08",
            "[UBL-DT-08]-[UBL-DT-08]-Scheme name attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@schemeAgencyName)
fn validate_ubl_dt_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-09",
            "[UBL-DT-09]-[UBL-DT-09]-Scheme agency name attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@schemeDataURI)
fn validate_ubl_dt_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-10",
            "[UBL-DT-10]-[UBL-DT-10]-Scheme data uri attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@schemeURI)
fn validate_ubl_dt_11(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-11",
            "[UBL-DT-11]-[UBL-DT-11]-Scheme uri attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@format)
fn validate_ubl_dt_12(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-12",
            "[UBL-DT-12]-[UBL-DT-12]-Format attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@unitCodeListIdentifier)
fn validate_ubl_dt_13(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-13",
            "[UBL-DT-13]-[UBL-DT-13]-Unit code list identifier attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@unitCodeListAgencyIdentifier)
fn validate_ubl_dt_14(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-14",
            "[UBL-DT-14]-[UBL-DT-14]-Unit code list agency identifier attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@unitCodeListAgencyName)
fn validate_ubl_dt_15(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-15",
            "[UBL-DT-15]-[UBL-DT-15]-Unit code list agency name attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@listAgencyName)
fn validate_ubl_dt_16(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-16",
            "[UBL-DT-16]-[UBL-DT-16]-List agency name attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@listName)
fn validate_ubl_dt_17(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-17",
            "[UBL-DT-17]-[UBL-DT-17]-List name attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: count(//@name) - count(//cbc:PaymentMeansCode/@name) <= 0
fn validate_ubl_dt_18(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-18",
            "[UBL-DT-18]-[UBL-DT-18]-Name attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@languageID)
fn validate_ubl_dt_19(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-19",
            "[UBL-DT-19]-[UBL-DT-19]-Language identifier attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@listURI)
fn validate_ubl_dt_20(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-20",
            "[UBL-DT-20]-[UBL-DT-20]-List uri attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@listSchemeURI)
fn validate_ubl_dt_21(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-21",
            "[UBL-DT-21]-[UBL-DT-21]-List scheme uri attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@languageLocaleID)
fn validate_ubl_dt_22(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-22",
            "[UBL-DT-22]-[UBL-DT-22]-Language local identifier attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@uri)
fn validate_ubl_dt_23(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-23",
            "[UBL-DT-23]-[UBL-DT-23]-Uri attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@currencyCodeListVersionID)
fn validate_ubl_dt_24(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-24",
            "[UBL-DT-24]-[UBL-DT-24]-Currency code list version id should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@characterSetCode)
fn validate_ubl_dt_25(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-25",
            "[UBL-DT-25]-[UBL-DT-25]-CharacterSetCode attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@encodingCode)
fn validate_ubl_dt_26(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-26",
            "[UBL-DT-26]-[UBL-DT-26]-EncodingCode attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@schemeAgencyID)
fn validate_ubl_dt_27(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-27",
            "[UBL-DT-27]-[UBL-DT-27]-Scheme Agency ID attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: not(//@listAgencyID)
fn validate_ubl_dt_28(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Warning(BusinessRuleViolation::new(
            "UBL-DT-28",
            "[UBL-DT-28]-[UBL-DT-28]-List Agency ID attribute should not be present",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:ContractDocumentReference/cbc:ID) <= 1)
fn validate_ubl_sr_01(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-01",
            "[UBL-SR-01]-[UBL-SR-01]-Contract identifier shall occur maximum once.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:ReceiptDocumentReference/cbc:ID) <= 1)
fn validate_ubl_sr_02(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-02",
            "[UBL-SR-02]-[UBL-SR-02]-Receive advice identifier shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:DespatchDocumentReference/cbc:ID) <= 1)
fn validate_ubl_sr_03(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-03",
            "[UBL-SR-03]-[UBL-SR-03]-Despatch advice identifier shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:AdditionalDocumentReference[cbc:DocumentTypeCode='130']/cbc:ID) <= 1)
fn validate_ubl_sr_04(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-04",
            "[UBL-SR-04]-[UBL-SR-04]-Invoice object identifier shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:PaymentTerms/cbc:Note) <= 1)
fn validate_ubl_sr_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-05",
            "[UBL-SR-05]-[UBL-SR-05]-Payment terms shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:InvoicePeriod) <= 1)
fn validate_ubl_sr_08(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-08",
            "[UBL-SR-08]-[UBL-SR-08]-Invoice period shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:RegistrationName) <= 1)
fn validate_ubl_sr_09(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-09",
            "[UBL-SR-09]-[UBL-SR-09]-Seller name shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:AccountingSupplierParty/cac:Party/cac:PartyName/cbc:Name) <= 1)
fn validate_ubl_sr_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-10",
            "[UBL-SR-10]-[UBL-SR-10]-Seller trader name shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:CompanyID) <= 1)
fn validate_ubl_sr_11(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-11",
            "[UBL-SR-11]-[UBL-SR-11]-Seller legal registration identifier shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/upper-case(cbc:ID)='VAT']/cbc:CompanyID) <= 1)
fn validate_ubl_sr_12(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-12",
            "[UBL-SR-12]-[UBL-SR-12]-Seller VAT identifier shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:AccountingSupplierParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/upper-case(cbc:ID)!='VAT']/cbc:CompanyID) <= 1)
fn validate_ubl_sr_13(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-13",
            "[UBL-SR-13]-[UBL-SR-13]-Seller tax registration shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:CompanyLegalForm) <= 1)
fn validate_ubl_sr_14(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-14",
            "[UBL-SR-14]-[UBL-SR-14]-Seller additional legal information shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:RegistrationName) <= 1)
fn validate_ubl_sr_15(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-15",
            "[UBL-SR-15]-[UBL-SR-15]-Buyer name shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:AccountingCustomerParty/cac:Party/cac:PartyIdentification/cbc:ID) <= 1)
fn validate_ubl_sr_16(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-16",
            "[UBL-SR-16]-[UBL-SR-16]-Buyer identifier shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:AccountingCustomerParty/cac:Party/cac:PartyLegalEntity/cbc:CompanyID) <= 1)
fn validate_ubl_sr_17(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-17",
            "[UBL-SR-17]-[UBL-SR-17]-Buyer legal registration identifier shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:AccountingCustomerParty/cac:Party/cac:PartyTaxScheme[cac:TaxScheme/upper-case(cbc:ID)='VAT']/cbc:CompanyID) <= 1)
fn validate_ubl_sr_18(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-18",
            "[UBL-SR-18]-[UBL-SR-18]-Buyer VAT identifier shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:Delivery) <= 1)
fn validate_ubl_sr_24(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-24",
            "[UBL-SR-24]-[UBL-SR-24]-Deliver to information shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(//cac:PartyIdentification/cbc:ID[upper-case(@schemeID) = 'SEPA']) <= 1)
fn validate_ubl_sr_29(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-29",
            "[UBL-SR-29]-[UBL-SR-29]-Bank creditor reference shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:ProjectReference/cbc:ID) <= 1)
fn validate_ubl_sr_39(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-39",
            "[UBL-SR-39]-[UBL-SR-39]-Project reference shall occur maximum once.",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:AccountingCustomerParty/cac:Party/cac:PartyName/cbc:Name) <= 1)
fn validate_ubl_sr_40(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-40",
            "[UBL-SR-40]-[UBL-SR-40]-Buyer trade name shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: count(//cbc:PaymentID[not(preceding::cbc:PaymentID/. = .)]) <= 1
fn validate_ubl_sr_44(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-44",
            "[UBL-SR-44]-[UBL-SR-44]-An Invoice may only have one unique PaymentID, but the PaymentID may be used for multiple PaymentMeans",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:PaymentMeans/cbc:PaymentDueDate) <=1)
fn validate_ubl_sr_45(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-45",
            "[UBL-SR-45]-[UBL-SR-45]-Due Date shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:PaymentMeans/cbc:PaymentMeansCode/@name) <=1)
fn validate_ubl_sr_46(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-46",
            "[UBL-SR-46]-[UBL-SR-46]-Payment means text shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: count(//cbc:PaymentMeansCode[not(preceding::cbc:PaymentMeansCode/. = .)]) <= 1
fn validate_ubl_sr_47(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-47",
            "[UBL-SR-47]-[UBL-SR-47]-When there are more than one payment means code, they shall be equal",
        )));
    }
    Ok(())
}

// Context: /ubl:Invoice | /cn:CreditNote
// Test: (count(cac:InvoicePeriod/cbc:DescriptionCode) <=1)
fn validate_ubl_sr_49(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-49",
            "[UBL-SR-49]-[UBL-SR-49]-Value tax point date shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: (count(cbc:Note) <= 1)
fn validate_ubl_sr_34(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-34",
            "[UBL-SR-34]-[UBL-SR-34]-Invoice line note shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: (count(cac:OrderLineReference/cbc:LineID) <= 1)
fn validate_ubl_sr_35(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-35",
            "[UBL-SR-35]-[UBL-SR-35]-Referenced purchase order line identifier shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: (count(cac:InvoicePeriod) <= 1)
fn validate_ubl_sr_36(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-36",
            "[UBL-SR-36]-[UBL-SR-36]-Invoice line period shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: (count(cac:Price/cac:AllowanceCharge/cbc:Amount) <= 1)
fn validate_ubl_sr_37(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-37",
            "[UBL-SR-37]-[UBL-SR-37]-Item price discount shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: count(cac:Item/cac:ClassifiedTaxCategory) = 1
fn validate_ubl_sr_48(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-48",
            "[UBL-SR-48]-[UBL-SR-48]-Invoice lines shall have one and only one classified tax category.",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: count(cac:Item/cbc:Description) <= 1
fn validate_ubl_sr_50(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-50",
            "[UBL-SR-50]-[UBL-SR-50]-Item description shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:InvoiceLine | cac:CreditNoteLine
// Test: count(cac:DocumentReference) <= 1
fn validate_ubl_sr_52(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-52",
            "[UBL-SR-52]-[UBL-SR-52]-Document reference shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:PayeeParty
// Test: (count(cac:PartyName/cbc:Name) <= 1) and ((cac:PartyName/cbc:Name) != (../cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:RegistrationName))
fn validate_ubl_sr_19(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-19",
            "[UBL-SR-19]-[UBL-SR-19]-Payee name shall occur maximum once, if the Payee is different from the Seller",
        )));
    }
    Ok(())
}

// Context: cac:PayeeParty
// Test: (count(cac:PartyIdentification/cbc:ID[upper-case(@schemeID) != 'SEPA']) <= 1) and ((cac:PartyName/cbc:Name) != (../cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:RegistrationName))
fn validate_ubl_sr_20(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-20",
            "[UBL-SR-20]-[UBL-SR-20]-Payee identifier shall occur maximum once, if the Payee is different from the Seller",
        )));
    }
    Ok(())
}

// Context: cac:PayeeParty
// Test: (count(cac:PartyLegalEntity/cbc:CompanyID) <= 1) and ((cac:PartyName/cbc:Name) != (../cac:AccountingSupplierParty/cac:Party/cac:PartyLegalEntity/cbc:RegistrationName))
fn validate_ubl_sr_21(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-21",
            "[UBL-SR-21]-[UBL-SR-21]-Payee legal registration identifier shall occur maximum once, if the Payee is different from the Seller",
        )));
    }
    Ok(())
}

// Context: cac:PaymentMeans
// Test: (count(cbc:PaymentID) <= 1)
fn validate_ubl_sr_26(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-26",
            "[UBL-SR-26]-[UBL-SR-26]-Payment reference shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:PaymentMeans
// Test: (count(cbc:PaymentMeansCode) <= 1)
fn validate_ubl_sr_27(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-27",
            "[UBL-SR-27]-[UBL-SR-27]-Payment means text shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:PaymentMeans
// Test: (count(cac:PaymentMandate/cbc:ID) <= 1)
fn validate_ubl_sr_28(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-28",
            "[UBL-SR-28]-[UBL-SR-28]-Mandate reference identifier shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:BillingReference
// Test: (count(cac:InvoiceDocumentReference) <= 1)
fn validate_ubl_sr_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-06",
            "[UBL-SR-06]-[UBL-SR-06]-Preceding invoice reference shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cac:BillingReference
// Test: (cac:InvoiceDocumentReference/cbc:ID)
fn validate_ubl_sr_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-07",
            "[UBL-SR-07]-[UBL-SR-07]-If there is a preceding invoice reference, the preceding invoice number shall be present",
        )));
    }
    Ok(())
}

// Context: cac:TaxRepresentativeParty
// Test: (count(cac:Party/cac:PartyName/cbc:Name) <= 1)
fn validate_ubl_sr_22(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-22",
            "[UBL-SR-22]-[UBL-SR-22]-Seller tax representative name shall occur maximum once, if the Seller has a tax representative",
        )));
    }
    Ok(())
}

// Context: cac:TaxRepresentativeParty
// Test: (count(cac:Party/cac:PartyTaxScheme/cbc:CompanyID) <= 1)
fn validate_ubl_sr_23(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-23",
            "[UBL-SR-23]-[UBL-SR-23]-Seller tax representative VAT identifier shall occur maximum once, if the Seller has a tax representative",
        )));
    }
    Ok(())
}

// Context: cac:TaxSubtotal
// Test: (count(cac:TaxCategory/cbc:TaxExemptionReason) <= 1)
fn validate_ubl_sr_32(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "UBL-SR-32",
            "[UBL-SR-32]-[UBL-SR-32]-VAT exemption reason text shall occur maximum once",
        )));
    }
    Ok(())
}

// Context: cbc:InvoiceTypeCode | cbc:CreditNoteTypeCode
// Test: (self::cbc:InvoiceTypeCode and ((not(contains(normalize-space(.), ' ')) and contains(' 71 80 81 82 84 102 130 202 203 204 211 218 219 295 325 326 331 380 382 383 384 385 386 387 388 389 390 393 394 395 456 457 527 553 575 623 633 751 780 817 870 875 876 877 935 ', concat(' ', normalize-space(.), ' '))))) or (self::cbc:CreditNoteTypeCode and ((not(contains(normalize-space(.), ' ')) and contains(' 81 83 261 262 296 308 381 396 420 458 532 ', concat(' ', normalize-space(.), ' ')))))
fn validate_br_cl_01(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-01",
            "[BR-CL-01]-The document type code MUST be coded by the invoice and credit note related code lists of UNTDID 1001.",
        )));
    }
    Ok(())
}

// Context: cbc:Amount | cbc:BaseAmount | cbc:PriceAmount | cbc:TaxAmount | cbc:TaxableAmount | cbc:LineExtensionAmount | cbc:TaxExclusiveAmount | cbc:TaxInclusiveAmount | cbc:AllowanceTotalAmount | cbc:ChargeTotalAmount | cbc:PrepaidAmount | cbc:PayableRoundingAmount | cbc:PayableAmount
// Test: ((not(contains(normalize-space(@currencyID), ' ')) and contains(' AED AFN ALL AMD ANG AOA ARS AUD AWG AZN BAM BBD BDT BGN BHD BIF BMD BND BOB BOV BRL BSD BTN BWP BYN BZD CAD CDF CHE CHF CHW CLF CLP CNY COP COU CRC CUC CUP CVE CZK DJF DKK DOP DZD EGP ERN ETB EUR FJD FKP GBP GEL GHS GIP GMD GNF GTQ GYD HKD HNL HTG HUF IDR ILS INR IQD IRR ISK JMD JOD JPY KES KGS KHR KMF KPW KRW KWD KYD KZT LAK LBP LKR LRD LSL LYD MAD MDL MGA MKD MMK MNT MOP MRO MUR MVR MWK MXN MXV MYR MZN NAD NGN NIO NOK NPR NZD OMR PAB PEN PGK PHP PKR PLN PYG QAR RON RSD RUB RWF SAR SBD SCR SDG SEK SGD SHP SLE SOS SRD SSP STD SVC SYP SZL THB TJS TMT TND TOP TRY TTD TWD TZS UAH UGX USD USN UYI UYU UZS VES VED VND VUV WST XAF XAG XAU XBA XBB XBC XBD XCD XDR XOF XPD XPF XPT XSU XTS XUA XXX YER ZAR ZMW ZWG ZWL ', concat(' ', normalize-space(@currencyID), ' '))))
fn validate_br_cl_03(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-03",
            "[BR-CL-03]-currencyID MUST be coded using ISO code list 4217 alpha-3",
        )));
    }
    Ok(())
}

// Context: cbc:DocumentCurrencyCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' AED AFN ALL AMD ANG AOA ARS AUD AWG AZN BAM BBD BDT BGN BHD BIF BMD BND BOB BOV BRL BSD BTN BWP BYN BZD CAD CDF CHE CHF CHW CLF CLP CNY COP COU CRC CUC CUP CVE CZK DJF DKK DOP DZD EGP ERN ETB EUR FJD FKP GBP GEL GHS GIP GMD GNF GTQ GYD HKD HNL HTG HUF IDR ILS INR IQD IRR ISK JMD JOD JPY KES KGS KHR KMF KPW KRW KWD KYD KZT LAK LBP LKR LRD LSL LYD MAD MDL MGA MKD MMK MNT MOP MRO MUR MVR MWK MXN MXV MYR MZN NAD NGN NIO NOK NPR NZD OMR PAB PEN PGK PHP PKR PLN PYG QAR RON RSD RUB RWF SAR SBD SCR SDG SEK SGD SHP SLE SOS SRD SSP STD SVC SYP SZL THB TJS TMT TND TOP TRY TTD TWD TZS UAH UGX USD USN UYI UYU UZS VES VED VND VUV WST XAF XAG XAU XBA XBB XBC XBD XCD XDR XOF XPD XPF XPT XSU XTS XUA XXX YER ZAR ZMW ZWG ZWL ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_04(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-04",
            "[BR-CL-04]-Invoice currency code MUST be coded using ISO code list 4217 alpha-3",
        )));
    }
    Ok(())
}

// Context: cbc:TaxCurrencyCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' AED AFN ALL AMD ANG AOA ARS AUD AWG AZN BAM BBD BDT BGN BHD BIF BMD BND BOB BOV BRL BSD BTN BWP BYN BZD CAD CDF CHE CHF CHW CLF CLP CNY COP COU CRC CUC CUP CVE CZK DJF DKK DOP DZD EGP ERN ETB EUR FJD FKP GBP GEL GHS GIP GMD GNF GTQ GYD HKD HNL HTG HUF IDR ILS INR IQD IRR ISK JMD JOD JPY KES KGS KHR KMF KPW KRW KWD KYD KZT LAK LBP LKR LRD LSL LYD MAD MDL MGA MKD MMK MNT MOP MRO MUR MVR MWK MXN MXV MYR MZN NAD NGN NIO NOK NPR NZD OMR PAB PEN PGK PHP PKR PLN PYG QAR RON RSD RUB RWF SAR SBD SCR SDG SEK SGD SHP SLE SOS SRD SSP STD SVC SYP SZL THB TJS TMT TND TOP TRY TTD TWD TZS UAH UGX USD USN UYI UYU UZS VES VED VND VUV WST XAF XAG XAU XBA XBB XBC XBD XCD XDR XOF XPD XPF XPT XSU XTS XUA XXX YER ZAR ZMW ZWG ZWL ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_05(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-05",
            "[BR-CL-05]-Tax currency code MUST be coded using ISO code list 4217 alpha-3",
        )));
    }
    Ok(())
}

// Context: cac:InvoicePeriod/cbc:DescriptionCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' 3 35 432 ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_06(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-06",
            "[BR-CL-06]-Value added tax point date code MUST be coded using a restriction of UNTDID 2005.",
        )));
    }
    Ok(())
}

// Context: cac:AdditionalDocumentReference[cbc:DocumentTypeCode = '130']/cbc:ID[@schemeID] | cac:DocumentReference[cbc:DocumentTypeCode = '130']/cbc:ID[@schemeID]
// Test: ((not(contains(normalize-space(@schemeID), ' ')) and contains(' AAA AAB AAC AAD AAE AAF AAG AAH AAI AAJ AAK AAL AAM AAN AAO AAP AAQ AAR AAS AAT AAU AAV AAW AAX AAY AAZ ABA ABB ABC ABD ABE ABF ABG ABH ABI ABJ ABK ABL ABM ABN ABO ABP ABQ ABR ABS ABT ABU ABV ABW ABX ABY ABZ AC ACA ACB ACC ACD ACE ACF ACG ACH ACI ACJ ACK ACL ACN ACO ACP ACQ ACR ACT ACU ACV ACW ACX ACY ACZ ADA ADB ADC ADD ADE ADF ADG ADI ADJ ADK ADL ADM ADN ADO ADP ADQ ADT ADU ADV ADW ADX ADY ADZ AE AEA AEB AEC AED AEE AEF AEG AEH AEI AEJ AEK AEL AEM AEN AEO AEP AEQ AER AES AET AEU AEV AEW AEX AEY AEZ AF AFA AFB AFC AFD AFE AFF AFG AFH AFI AFJ AFK AFL AFM AFN AFO AFP AFQ AFR AFS AFT AFU AFV AFW AFX AFY AFZ AGA AGB AGC AGD AGE AGF AGG AGH AGI AGJ AGK AGL AGM AGN AGO AGP AGQ AGR AGS AGT AGU AGV AGW AGX AGY AGZ AHA AHB AHC AHD AHE AHF AHG AHH AHI AHJ AHK AHL AHM AHN AHO AHP AHQ AHR AHS AHT AHU AHV AHX AHY AHZ AIA AIB AIC AID AIE AIF AIG AIH AII AIJ AIK AIL AIM AIN AIO AIP AIQ AIR AIS AIT AIU AIV AIW AIX AIY AIZ AJA AJB AJC AJD AJE AJF AJG AJH AJI AJJ AJK AJL AJM AJN AJO AJP AJQ AJR AJS AJT AJU AJV AJW AJX AJY AJZ AKA AKB AKC AKD AKE AKF AKG AKH AKI AKJ AKK AKL AKM AKN AKO AKP AKQ AKR AKS AKT AKU AKV AKW AKX AKY AKZ ALA ALB ALC ALD ALE ALF ALG ALH ALI ALJ ALK ALL ALM ALN ALO ALP ALQ ALR ALS ALT ALU ALV ALW ALX ALY ALZ AMA AMB AMC AMD AME AMF AMG AMH AMI AMJ AMK AML AMM AMN AMO AMP AMQ AMR AMS AMT AMU AMV AMW AMX AMY AMZ ANA ANB ANC AND ANE ANF ANG ANH ANI ANJ ANK ANL ANM ANN ANO ANP ANQ ANR ANS ANT ANU ANV ANW ANX ANY AOA AOD AOE AOF AOG AOH AOI AOJ AOK AOL AOM AON AOO AOP AOQ AOR AOS AOT AOU AOV AOW AOX AOY AOZ AP APA APB APC APD APE APF APG APH API APJ APK APL APM APN APO APP APQ APR APS APT APU APV APW APX APY APZ AQA AQB AQC AQD AQE AQF AQG AQH AQI AQJ AQK AQL AQM AQN AQO AQP AQQ AQR AQS AQT AQU AQV AQW AQX AQY AQZ ARA ARB ARC ARD ARE ARF ARG ARH ARI ARJ ARK ARL ARM ARN ARO ARP ARQ ARR ARS ART ARU ARV ARW ARX ARY ARZ ASA ASB ASC ASD ASE ASF ASG ASH ASI ASJ ASK ASL ASM ASN ASO ASP ASQ ASR ASS AST ASU ASV ASW ASX ASY ASZ ATA ATB ATC ATD ATE ATF ATG ATH ATI ATJ ATK ATL ATM ATN ATO ATP ATQ ATR ATS ATT ATU ATV ATW ATX ATY ATZ AU AUA AUB AUC AUD AUE AUF AUG AUH AUI AUJ AUK AUL AUM AUN AUO AUP AUQ AUR AUS AUT AUU AUV AUW AUX AUY AUZ AV AVA AVB AVC AVD AVE AVF AVG AVH AVI AVJ AVK AVL AVM AVN AVO AVP AVQ AVR AVS AVT AVU AVV AVW AVX AVY AVZ AWA AWB AWC AWD AWE AWF AWG AWH AWI AWJ AWK AWL AWM AWN AWO AWP AWQ AWR AWS AWT AWU AWV AWW AWX AWY AWZ AXA AXB AXC AXD AXE AXF AXG AXH AXI AXJ AXK AXL AXM AXN AXO AXP AXQ AXR AXS BA BC BD BE BH BM BN BO BR BT BTP BW CAS CAT CAU CAV CAW CAX CAY CAZ CBA CBB CD CEC CED CFE CFF CFO CG CH CK CKN CM CMR CN CNO COF CP CR CRN CS CST CT CU CV CW CZ DA DAN DB DI DL DM DQ DR EA EB ED EE EEP EI EN EQ ER ERN ET EX FC FF FI FLW FN FO FS FT FV FX GA GC GD GDN GN HS HWB IA IB ICA ICE ICO II IL INB INN INO IP IS IT IV JB JE LA LAN LAR LB LC LI LO LRC LS MA MB MF MG MH MR MRN MS MSS MWB NA NF OH OI ON OP OR PB PC PD PE PF PI PK PL POR PP PQ PR PS PW PY RA RC RCN RE REN RF RR RT SA SB SD SE SEA SF SH SI SM SN SP SQ SRN SS STA SW SZ TB TCR TE TF TI TIN TL TN TP UAR UC UCN UN UO URI VA VC VGR VM VN VON VOR VP VR VS VT VV WE WM WN WR WS WY XA XC XP ZZZ ', concat(' ', normalize-space(@schemeID), ' '))))
fn validate_br_cl_07(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-07",
            "[BR-CL-07]-Object identifier identification scheme identifier MUST be coded using a restriction of UNTDID 1153.",
        )));
    }
    Ok(())
}

// Context: cac:PartyIdentification/cbc:ID[@schemeID]
// Test: ((not(contains(normalize-space(@schemeID), ' ')) and contains(' 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048 0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0093 0094 0095 0096 0097 0098 0099 0100 0101 0102 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144 0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192 0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 ', concat(' ', normalize-space(@schemeID), ' '))))  or ((not(contains(normalize-space(@schemeID), ' ')) and contains(' SEPA ', concat(' ', normalize-space(@schemeID), ' '))) and ((ancestor::cac:AccountingSupplierParty) or (ancestor::cac:PayeeParty)))
fn validate_br_cl_10(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-10",
            "[BR-CL-10]-Any identifier identification scheme identifier MUST be coded using one of the ISO 6523 ICD list.",
        )));
    }
    Ok(())
}

// Context: cac:PartyLegalEntity/cbc:CompanyID[@schemeID]
// Test: ((not(contains(normalize-space(@schemeID), ' ')) and contains(' 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048 0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0093 0094 0095 0096 0097 0098 0099 0100 0101 0102 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144 0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192 0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 ', concat(' ', normalize-space(@schemeID), ' '))))
fn validate_br_cl_11(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-11",
            "[BR-CL-11]-Any registration identifier identification scheme identifier MUST be coded using one of the ISO 6523 ICD list.",
        )));
    }
    Ok(())
}

// Context: cac:CommodityClassification/cbc:ItemClassificationCode[@listID]
// Test: ((not(contains(normalize-space(@listID), ' ')) and contains(' AA AB AC AD AE AF AG AH AI AJ AK AL AM AN AO AP AQ AR AS AT AU AV AW AX AY AZ BA BB BC BD BE BF BG BH BI BJ BK BL BM BN BO BP BQ BR BS BT BU BV BW BX BY BZ CC CG CL CR CV DR DW EC EF EMD EN FS GB GN GMN GS HS IB IN IS IT IZ MA MF MN MP NB ON PD PL PO PV QS RC RN RU RY SA SG SK SN SRS SRT SRU SRV SRW SRX SRY SRZ SS SSA SSB SSC SSD SSE SSF SSG SSH SSI SSJ SSK SSL SSM SSN SSO SSP SSQ SSR SSS SST SSU SSV SSW SSX SSY SSZ ST STA STB STC STD STE STF STG STH STI STJ STK STL STM STN STO STP STQ STR STS STT STU STV STW STX STY STZ SUA SUB SUC SUD SUE SUF SUG SUH SUI SUJ SUK SUL SUM TG TSN TSO TSP TSQ TSR TSS TST TSU UA UP VN VP VS VX ZZZ ', concat(' ', normalize-space(@listID), ' '))))
fn validate_br_cl_13(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-13",
            "[BR-CL-13]-Item classification identifier identification scheme identifier MUST be
      coded using one of the UNTDID 7143 list.",
        )));
    }
    Ok(())
}

// Context: cac:Country/cbc:IdentificationCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' 1A AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS XI YE YT ZA ZM ZW ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_14(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-14",
            "[BR-CL-14]-Country codes in an invoice MUST be coded using ISO code list 3166-1",
        )));
    }
    Ok(())
}

// Context: cac:OriginCountry/cbc:IdentificationCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' 1A AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS XI YE YT ZA ZM ZW ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_15(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-15",
            "[BR-CL-15]-Country codes in an invoice MUST be coded using ISO code list 3166-1",
        )));
    }
    Ok(())
}

// Context: cac:PaymentMeans/cbc:PaymentMeansCode
// Test: ( ( not(contains(normalize-space(.),' ')) and contains( ' 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63 64 65 66 67 68 69 70 74 75 76 77 78 91 92 93 94 95 96 97 98 ZZZ ',concat(' ',normalize-space(.),' ') ) ) )
fn validate_br_cl_16(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-16",
            "[BR-CL-16]-Payment means in an invoice MUST be coded using UNCL4461 code list",
        )));
    }
    Ok(())
}

// Context: cac:TaxCategory/cbc:ID
// Test: ( ( not(contains(normalize-space(.),' ')) and contains( ' AE L M E S Z G O K B ',concat(' ',normalize-space(.),' ') ) ) )
fn validate_br_cl_17(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-17",
            "[BR-CL-17]-Invoice tax categories MUST be coded using UNCL5305 code list",
        )));
    }
    Ok(())
}

// Context: cac:ClassifiedTaxCategory/cbc:ID
// Test: ( ( not(contains(normalize-space(.),' ')) and contains( ' AE L M E S Z G O K B ',concat(' ',normalize-space(.),' ') ) ) )
fn validate_br_cl_18(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-18",
            "[BR-CL-18]-Invoice tax categories MUST be coded using UNCL5305 code list",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator = false()]/cbc:AllowanceChargeReasonCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' 41 42 60 62 63 64 65 66 67 68 70 71 88 95 100 102 103 104 105 ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_19(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-19",
            "[BR-CL-19]-Coded allowance reasons MUST belong to the UNCL 5189 code list",
        )));
    }
    Ok(())
}

// Context: cac:AllowanceCharge[cbc:ChargeIndicator = true()]/cbc:AllowanceChargeReasonCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' AA AAA AAC AAD AAE AAF AAH AAI AAS AAT AAV AAY AAZ ABA ABB ABC ABD ABF ABK ABL ABN ABR ABS ABT ABU ACF ACG ACH ACI ACJ ACK ACL ACM ACS ADC ADE ADJ ADK ADL ADM ADN ADO ADP ADQ ADR ADT ADW ADY ADZ AEA AEB AEC AED AEF AEH AEI AEJ AEK AEL AEM AEN AEO AEP AES AET AEU AEV AEW AEX AEY AEZ AJ AU CA CAB CAD CAE CAF CAI CAJ CAK CAL CAM CAN CAO CAP CAQ CAR CAS CAT CAU CAV CAW CAX CAY CAZ CD CG CS CT DAB DAD DAC DAF DAG DAH DAI DAJ DAK DAL DAM DAN DAO DAP DAQ DL EG EP ER FAA FAB FAC FC FH FI GAA HAA HD HH IAA IAB ID IF IR IS KO L1 LA LAA LAB LF MAE MI ML NAA OA PA PAA PC PL PRV RAB RAC RAD RAF RE RF RH RV SA SAA SAD SAE SAI SG SH SM SU TAB TAC TT TV V1 V2 WH XAA YY ZZZ ', concat(' ', normalize-space(.), ' '))))
fn validate_br_cl_20(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-20",
            "[BR-CL-20]-Coded charge reasons MUST belong to the UNCL 7161 code list",
        )));
    }
    Ok(())
}

// Context: cac:StandardItemIdentification/cbc:ID[@schemeID]
// Test: ((not(contains(normalize-space(@schemeID), ' ')) and contains(' 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048 0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0093 0094 0095 0096 0097 0098 0099 0100 0101 0102 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144 0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192 0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 ', concat(' ', normalize-space(@schemeID), ' '))))
fn validate_br_cl_21(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-21",
            "[BR-CL-21]-Item standard identifier scheme identifier MUST belong to the ISO 6523 ICD code list",
        )));
    }
    Ok(())
}

// Context: cbc:TaxExemptionReasonCode
// Test: ((not(contains(normalize-space(.), ' ')) and contains(' VATEX-EU-79-C VATEX-EU-132 VATEX-EU-132-1A VATEX-EU-132-1B VATEX-EU-132-1C VATEX-EU-132-1D VATEX-EU-132-1E VATEX-EU-132-1F VATEX-EU-132-1G VATEX-EU-132-1H VATEX-EU-132-1I VATEX-EU-132-1J VATEX-EU-132-1K VATEX-EU-132-1L VATEX-EU-132-1M VATEX-EU-132-1N VATEX-EU-132-1O VATEX-EU-132-1P VATEX-EU-132-1Q VATEX-EU-143 VATEX-EU-143-1A VATEX-EU-143-1B VATEX-EU-143-1C VATEX-EU-143-1D VATEX-EU-143-1E VATEX-EU-143-1F VATEX-EU-143-1FA VATEX-EU-143-1G VATEX-EU-143-1H VATEX-EU-143-1I VATEX-EU-143-1J VATEX-EU-143-1K VATEX-EU-143-1L VATEX-EU-144 VATEX-EU-146-1E VATEX-EU-159 VATEX-EU-309 VATEX-EU-148 VATEX-EU-148-A VATEX-EU-148-B VATEX-EU-148-C VATEX-EU-148-D VATEX-EU-148-E VATEX-EU-148-F VATEX-EU-148-G VATEX-EU-151 VATEX-EU-151-1A VATEX-EU-151-1AA VATEX-EU-151-1B VATEX-EU-151-1C VATEX-EU-151-1D VATEX-EU-151-1E VATEX-EU-G VATEX-EU-O VATEX-EU-IC VATEX-EU-AE VATEX-EU-D VATEX-EU-F VATEX-EU-I VATEX-EU-J VATEX-FR-FRANCHISE VATEX-FR-CNWVAT ', concat(' ', normalize-space(upper-case(.)), ' '))))
fn validate_br_cl_22(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-22",
            "[BR-CL-22]-Tax exemption reason code identifier scheme identifier MUST belong to the CEF VATEX code list",
        )));
    }
    Ok(())
}

// Context: cbc:InvoicedQuantity[@unitCode] | cbc:BaseQuantity[@unitCode] | cbc:CreditedQuantity[@unitCode]
// Test: ((not(contains(normalize-space(@unitCode), ' ')) and contains(' 10 11 13 14 15 20 21 22 23 24 25 27 28 33 34 35 37 38 40 41 56 57 58 59 60 61 74 77 80 81 85 87 89 91 1I 2A 2B 2C 2G 2H 2I 2J 2K 2L 2M 2N 2P 2Q 2R 2U 2X 2Y 2Z 3B 3C 4C 4G 4H 4K 4L 4M 4N 4O 4P 4Q 4R 4T 4U 4W 4X 5A 5B 5E 5J A10 A11 A12 A13 A14 A15 A16 A17 A18 A19 A2 A20 A21 A22 A23 A24 A26 A27 A28 A29 A3 A30 A31 A32 A33 A34 A35 A36 A37 A38 A39 A4 A40 A41 A42 A43 A44 A45 A47 A48 A49 A5 A53 A54 A55 A56 A59 A6 A68 A69 A7 A70 A71 A73 A74 A75 A76 A8 A84 A85 A86 A87 A88 A89 A9 A90 A91 A93 A94 A95 A96 A97 A98 A99 AA AB ACR ACT AD AE AH AI AK AL AMH AMP ANN APZ AQ AS ASM ASU ATM AWG AY AZ B1 B10 B11 B12 B13 B14 B15 B16 B17 B18 B19 B20 B21 B22 B23 B24 B25 B26 B27 B28 B29 B3 B30 B31 B32 B33 B34 B35 B4 B41 B42 B43 B44 B45 B46 B47 B48 B49 B50 B52 B53 B54 B55 B56 B57 B58 B59 B60 B61 B62 B63 B64 B66 B67 B68 B69 B7 B70 B71 B72 B73 B74 B75 B76 B77 B78 B79 B8 B80 B81 B82 B83 B84 B85 B86 B87 B88 B89 B90 B91 B92 B93 B94 B95 B96 B97 B98 B99 BAR BB BFT BHP BIL BLD BLL BP BPM BQL BTU BUA BUI C0 C10 C11 C12 C13 C14 C15 C16 C17 C18 C19 C20 C21 C22 C23 C24 C25 C26 C27 C28 C29 C3 C30 C31 C32 C33 C34 C35 C36 C37 C38 C39 C40 C41 C42 C43 C44 C45 C46 C47 C48 C49 C50 C51 C52 C53 C54 C55 C56 C57 C58 C59 C60 C61 C62 C63 C64 C65 C66 C67 C68 C69 C7 C70 C71 C72 C73 C74 C75 C76 C78 C79 C8 C80 C81 C82 C83 C84 C85 C86 C87 C88 C89 C9 C90 C91 C92 C93 C94 C95 C96 C97 C99 CCT CDL CEL CEN CG CGM CKG CLF CLT CMK CMQ CMT CNP CNT COU CTG CTM CTN CUR CWA CWI D03 D04 D1 D10 D11 D12 D13 D15 D16 D17 D18 D19 D2 D20 D21 D22 D23 D24 D25 D26 D27 D29 D30 D31 D32 D33 D34 D36 D41 D42 D43 D44 D45 D46 D47 D48 D49 D5 D50 D51 D52 D53 D54 D55 D56 D57 D58 D59 D6 D60 D61 D62 D63 D65 D68 D69 D73 D74 D77 D78 D80 D81 D82 D83 D85 D86 D87 D88 D89 D91 D93 D94 D95 DAA DAD DAY DB DBM DBW DD DEC DG DJ DLT DMA DMK DMO DMQ DMT DN DPC DPR DPT DRA DRI DRL DT DTN DWT DZN DZP E01 E07 E08 E09 E10 E12 E14 E15 E16 E17 E18 E19 E20 E21 E22 E23 E25 E27 E28 E30 E31 E32 E33 E34 E35 E36 E37 E38 E39 E4 E40 E41 E42 E43 E44 E45 E46 E47 E48 E49 E50 E51 E52 E53 E54 E55 E56 E57 E58 E59 E60 E61 E62 E63 E64 E65 E66 E67 E68 E69 E70 E71 E72 E73 E74 E75 E76 E77 E78 E79 E80 E81 E82 E83 E84 E85 E86 E87 E88 E89 E90 E91 E92 E93 E94 E95 E96 E97 E98 E99 EA EB EQ F01 F02 F03 F04 F05 F06 F07 F08 F10 F11 F12 F13 F14 F15 F16 F17 F18 F19 F20 F21 F22 F23 F24 F25 F26 F27 F28 F29 F30 F31 F32 F33 F34 F35 F36 F37 F38 F39 F40 F41 F42 F43 F44 F45 F46 F47 F48 F49 F50 F51 F52 F53 F54 F55 F56 F57 F58 F59 F60 F61 F62 F63 F64 F65 F66 F67 F68 F69 F70 F71 F72 F73 F74 F75 F76 F77 F78 F79 F80 F81 F82 F83 F84 F85 F86 F87 F88 F89 F90 F91 F92 F93 F94 F95 F96 F97 F98 F99 FAH FAR FBM FC FF FH FIT FL FNU FOT FP FR FS FTK FTQ G01 G04 G05 G06 G08 G09 G10 G11 G12 G13 G14 G15 G16 G17 G18 G19 G2 G20 G21 G23 G24 G25 G26 G27 G28 G29 G3 G30 G31 G32 G33 G34 G35 G36 G37 G38 G39 G40 G41 G42 G43 G44 G45 G46 G47 G48 G49 G50 G51 G52 G53 G54 G55 G56 G57 G58 G59 G60 G61 G62 G63 G64 G65 G66 G67 G68 G69 G70 G71 G72 G73 G74 G75 G76 G77 G78 G79 G80 G81 G82 G83 G84 G85 G86 G87 G88 G89 G90 G91 G92 G93 G94 G95 G96 G97 G98 G99 GB GBQ GDW GE GF GFI GGR GIA GIC GII GIP GJ GL GLD GLI GLL GM GO GP GQ GRM GRN GRO GV GWH H03 H04 H05 H06 H07 H08 H09 H10 H11 H12 H13 H14 H15 H16 H18 H19 H20 H21 H22 H23 H24 H25 H26 H27 H28 H29 H30 H31 H32 H33 H34 H35 H36 H37 H38 H39 H40 H41 H42 H43 H44 H45 H46 H47 H48 H49 H50 H51 H52 H53 H54 H55 H56 H57 H58 H59 H60 H61 H62 H63 H64 H65 H66 H67 H68 H69 H70 H71 H72 H73 H74 H75 H76 H77 H79 H80 H81 H82 H83 H84 H85 H87 H88 H89 H90 H91 H92 H93 H94 H95 H96 H98 H99 HA HAD HBA HBX HC HDW HEA HGM HH HIU HKM HLT HM HMO HMQ HMT HPA HTZ HUR HWE IA IE INH INK INQ ISD IU IUG IV J10 J12 J13 J14 J15 J16 J17 J18 J19 J2 J20 J21 J22 J23 J24 J25 J26 J27 J28 J29 J30 J31 J32 J33 J34 J35 J36 J38 J39 J40 J41 J42 J43 J44 J45 J46 J47 J48 J49 J50 J51 J52 J53 J54 J55 J56 J57 J58 J59 J60 J61 J62 J63 J64 J65 J66 J67 J68 J69 J70 J71 J72 J73 J74 J75 J76 J78 J79 J81 J82 J83 J84 J85 J87 J90 J91 J92 J93 J95 J96 J97 J98 J99 JE JK JM JNT JOU JPS JWL K1 K10 K11 K12 K13 K14 K15 K16 K17 K18 K19 K2 K20 K21 K22 K23 K26 K27 K28 K3 K30 K31 K32 K33 K34 K35 K36 K37 K38 K39 K40 K41 K42 K43 K45 K46 K47 K48 K49 K50 K51 K52 K53 K54 K55 K58 K59 K6 K60 K61 K62 K63 K64 K65 K66 K67 K68 K69 K70 K71 K73 K74 K75 K76 K77 K78 K79 K80 K81 K82 K83 K84 K85 K86 K87 K88 K89 K90 K91 K92 K93 K94 K95 K96 K97 K98 K99 KA KAT KB KBA KCC KDW KEL KGM KGS KHY KHZ KI KIC KIP KJ KJO KL KLK KLX KMA KMH KMK KMQ KMT KNI KNM KNS KNT KO KPA KPH KPO KPP KR KSD KSH KT KTN KUR KVA KVR KVT KW KWH KWN KWO KWS KWT KWY KX L10 L11 L12 L13 L14 L15 L16 L17 L18 L19 L2 L20 L21 L23 L24 L25 L26 L27 L28 L29 L30 L31 L32 L33 L34 L35 L36 L37 L38 L39 L40 L41 L42 L43 L44 L45 L46 L47 L48 L49 L50 L51 L52 L53 L54 L55 L56 L57 L58 L59 L60 L63 L64 L65 L66 L67 L68 L69 L70 L71 L72 L73 L74 L75 L76 L77 L78 L79 L80 L81 L82 L83 L84 L85 L86 L87 L88 L89 L90 L91 L92 L93 L94 L95 L96 L98 L99 LA LAC LBR LBT LD LEF LF LH LK LM LN LO LP LPA LR LS LTN LTR LUB LUM LUX LY M1 M10 M11 M12 M13 M14 M15 M16 M17 M18 M19 M20 M21 M22 M23 M24 M25 M26 M27 M29 M30 M31 M32 M33 M34 M35 M36 M37 M38 M39 M4 M40 M41 M42 M43 M44 M45 M46 M47 M48 M49 M5 M50 M51 M52 M53 M55 M56 M57 M58 M59 M60 M61 M62 M63 M64 M65 M66 M67 M68 M69 M7 M70 M71 M72 M73 M74 M75 M76 M77 M78 M79 M80 M81 M82 M83 M84 M85 M86 M87 M88 M89 M9 M90 M91 M92 M93 M94 M95 M96 M97 M98 M99 MAH MAL MAM MAR MAW MBE MBF MBR MC MCU MD MGM MHZ MIK MIL MIN MIO MIU MKD MKM MKW MLD MLT MMK MMQ MMT MND MNJ MON MPA MQD MQH MQM MQS MQW MRD MRM MRW MSK MTK MTQ MTR MTS MTZ MVA MWH N1 N10 N11 N12 N13 N14 N15 N16 N17 N18 N19 N20 N21 N22 N23 N24 N25 N26 N27 N28 N29 N3 N30 N31 N32 N33 N34 N35 N36 N37 N38 N39 N40 N41 N42 N43 N44 N45 N46 N47 N48 N49 N50 N51 N52 N53 N54 N55 N56 N57 N58 N59 N60 N61 N62 N63 N64 N65 N66 N67 N68 N69 N70 N71 N72 N73 N74 N75 N76 N77 N78 N79 N80 N81 N82 N83 N84 N85 N86 N87 N88 N89 N90 N91 N92 N93 N94 N95 N96 N97 N98 N99 NA NAR NCL NEW NF NIL NIU NL NM3 NMI NMP NPT NT NTU NU NX OA ODE ODG ODK ODM OHM ON ONZ OPM OT OZA OZI P1 P10 P11 P12 P13 P14 P15 P16 P17 P18 P19 P2 P20 P21 P22 P23 P24 P25 P26 P27 P28 P29 P30 P31 P32 P33 P34 P35 P36 P37 P38 P39 P40 P41 P42 P43 P44 P45 P46 P47 P48 P49 P5 P50 P51 P52 P53 P54 P55 P56 P57 P58 P59 P60 P61 P62 P63 P64 P65 P66 P67 P68 P69 P70 P71 P72 P73 P74 P75 P76 P77 P78 P79 P80 P81 P82 P83 P84 P85 P86 P87 P88 P89 P90 P91 P92 P93 P94 P95 P96 P97 P98 P99 PAL PD PFL PGL PI PLA PO PQ PR PS PTD PTI PTL PTN Q10 Q11 Q12 Q13 Q14 Q15 Q16 Q17 Q18 Q19 Q20 Q21 Q22 Q23 Q24 Q25 Q26 Q27 Q28 Q29 Q3 Q30 Q31 Q32 Q33 Q34 Q35 Q36 Q37 Q38 Q39 Q40 Q41 Q42 QA QAN QB QR QTD QTI QTL QTR R1 R9 RH RM ROM RP RPM RPS RT S3 S4 SAN SCO SCR SEC SET SG SIE SM3 SMI SQ SQR SR STC STI STK STL STN STW SW SX SYR T0 T3 TAH TAN TI TIC TIP TKM TMS TNE TP TPI TPR TQD TRL TST TTS U1 U2 UB UC VA VLT VP W2 WA WB WCD WE WEB WEE WG WHR WM WSD WTT X1 YDK YDQ YRD Z11 Z9 ZP ZZ X1A X1B X1D X1F X1G X1W X2C X3A X3H X43 X44 X4A X4B X4C X4D X4F X4G X4H X5H X5L X5M X6H X6P X7A X7B X8A X8B X8C XAA XAB XAC XAD XAE XAF XAG XAH XAI XAJ XAL XAM XAP XAT XAV XB4 XBA XBB XBC XBD XBE XBF XBG XBH XBI XBJ XBK XBL XBM XBN XBO XBP XBQ XBR XBS XBT XBU XBV XBW XBX XBY XBZ XCA XCB XCC XCD XCE XCF XCG XCH XCI XCJ XCK XCL XCM XCN XCO XCP XCQ XCR XCS XCT XCU XCV XCW XCX XCY XCZ XDA XDB XDC XDG XDH XDI XDJ XDK XDL XDM XDN XDP XDR XDS XDT XDU XDV XDW XDX XDY XEC XED XEE XEF XEG XEH XEI XEN XFB XFC XFD XFE XFI XFL XFO XFP XFR XFT XFW XFX XGB XGI XGL XGR XGU XGY XGZ XHA XHB XHC XHG XHN XHR XIA XIB XIC XID XIE XIF XIG XIH XIK XIL XIN XIZ XJB XJC XJG XJR XJT XJY XKG XKI XLE XLG XLT XLU XLV XLZ XMA XMB XMC XME XMR XMS XMT XMW XMX XNA XNE XNF XNG XNS XNT XNU XNV XO1 XO2 XO3 XO4 XO5 XO6 XO7 XO8 XO9 XOA XOB XOC XOD XOE XOF XOG XOH XOI XOJ XOK XOL XOM XON XOP XOQ XOR XOS XOT XOU XOV XOW XOX XOY XOZ XP1 XP2 XP3 XP4 XPA XPB XPC XPD XPE XPF XPG XPH XPI XPJ XPK XPL XPN XPO XPP XPR XPT XPU XPV XPX XPY XPZ XQA XQB XQC XQD XQF XQG XQH XQJ XQK XQL XQM XQN XQP XQQ XQR XQS XRD XRG XRJ XRK XRL XRO XRT XRZ XSA XSB XSC XSD XSE XSH XSI XSK XSL XSM XSO XSP XSS XST XSU XSV XSW XSX XSY XSZ XT1 XTB XTC XTD XTE XTG XTI XTK XTL XTN XTO XTR XTS XTT XTU XTV XTW XTY XTZ XUC XUN XVA XVG XVI XVK XVL XVN XVO XVP XVQ XVR XVS XVY XWA XWB XWC XWD XWF XWG XWH XWJ XWK XWL XWM XWN XWP XWQ XWR XWS XWT XWU XWV XWW XWX XWY XWZ XXA XXB XXC XXD XXF XXG XXH XXJ XXK XYA XYB XYC XYD XYF XYG XYH XYJ XYK XYL XYM XYN XYP XYQ XYR XYS XYT XYV XYW XYX XYY XYZ XZA XZB XZC XZD XZF XZG XZH XZJ XZK XZL XZM XZN XZP XZQ XZR XZS XZT XZU XZV XZW XZX XZY XZZ ', concat(' ', normalize-space(@unitCode), ' '))))
fn validate_br_cl_23(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-23",
            "[BR-CL-23]-Unit code MUST be coded according to the UN/ECE Recommendation 20 with
      Rec 21 extension",
        )));
    }
    Ok(())
}

// Context: cbc:EmbeddedDocumentBinaryObject[@mimeCode]
// Test: ((@mimeCode = 'application/pdf' or @mimeCode = 'image/png' or @mimeCode = 'image/jpeg' or @mimeCode = 'text/csv' or @mimeCode = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' or @mimeCode = 'application/vnd.oasis.opendocument.spreadsheet'))
fn validate_br_cl_24(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-24",
            "[BR-CL-24]-For Mime code in attribute use MIMEMediaType.",
        )));
    }
    Ok(())
}

// Context: cbc:EndpointID[@schemeID]
// Test: ((not(contains(normalize-space(@schemeID), ' ')) and contains(' 0002 0007 0009 0037 0060 0088 0096 0097 0106 0130 0135 0142 0147 0151 0170 0177 0183 0184 0188 0190 0191 0192 0193 0194 0195 0196 0198 0199 0200 0201 0202 0203 0204 0205 0208 0209 0210 0211 0212 0213 0215 0216 0217 0218 0219 0220 0221 0225 0230 0235 9901 9910 9913 9914 9915 9918 9919 9920 9922 9923 9924 9925 9926 9927 9928 9929 9930 9931 9932 9933 9934 9935 9936 9937 9938 9939 9940 9941 9942 9943 9944 9945 9946 9947 9948 9949 9950 9951 9952 9953 9957 9959 AN AQ AS AU EM ', concat(' ', normalize-space(@schemeID), ' '))))
fn validate_br_cl_25(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-25",
            "[BR-CL-25]-Endpoint identifier scheme identifier MUST belong to the CEF EAS code list",
        )));
    }
    Ok(())
}

// Context: cac:DeliveryLocation/cbc:ID[@schemeID]
// Test: ((not(contains(normalize-space(@schemeID), ' ')) and contains(' 0002 0003 0004 0005 0006 0007 0008 0009 0010 0011 0012 0013 0014 0015 0016 0017 0018 0019 0020 0021 0022 0023 0024 0025 0026 0027 0028 0029 0030 0031 0032 0033 0034 0035 0036 0037 0038 0039 0040 0041 0042 0043 0044 0045 0046 0047 0048 0049 0050 0051 0052 0053 0054 0055 0056 0057 0058 0059 0060 0061 0062 0063 0064 0065 0066 0067 0068 0069 0070 0071 0072 0073 0074 0075 0076 0077 0078 0079 0080 0081 0082 0083 0084 0085 0086 0087 0088 0089 0090 0091 0093 0094 0095 0096 0097 0098 0099 0100 0101 0102 0104 0105 0106 0107 0108 0109 0110 0111 0112 0113 0114 0115 0116 0117 0118 0119 0120 0121 0122 0123 0124 0125 0126 0127 0128 0129 0130 0131 0132 0133 0134 0135 0136 0137 0138 0139 0140 0141 0142 0143 0144 0145 0146 0147 0148 0149 0150 0151 0152 0153 0154 0155 0156 0157 0158 0159 0160 0161 0162 0163 0164 0165 0166 0167 0168 0169 0170 0171 0172 0173 0174 0175 0176 0177 0178 0179 0180 0183 0184 0185 0186 0187 0188 0189 0190 0191 0192 0193 0194 0195 0196 0197 0198 0199 0200 0201 0202 0203 0204 0205 0206 0207 0208 0209 0210 0211 0212 0213 0214 0215 0216 0217 0218 0219 0220 0221 0222 0223 0224 0225 0226 0227 0228 0229 0230 0231 0232 0233 0234 0235 0236 0237 0238 ', concat(' ', normalize-space(@schemeID), ' '))))
fn validate_br_cl_26(_invoice: &UblInvoice) -> Result<(), ValidationError> {
    if false {
        return Err(ValidationError::Fatal(BusinessRuleViolation::new(
            "BR-CL-26",
            "[BR-CL-26]-Delivery location identifier scheme identifier MUST belong to the ISO 6523 ICD code list",
        )));
    }
    Ok(())
}
