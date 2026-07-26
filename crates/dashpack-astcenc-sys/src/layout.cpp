// SPDX-License-Identifier: MIT
//
// Not part of astcenc. This file reports the memory layout that the vendored
// astcenc.h actually produces, so the hand-written `astcenc_config` in lib.rs
// can be checked against the compiler instead of trusted.
//
// astcenc_config is the one type this crate declares that a mistake in would be
// silent: it is passed by pointer into astcenc_context_alloc, so a field in the
// wrong place reads a neighbouring value rather than failing. Every other type
// crossing the boundary is either opaque or small enough to read at a glance.

#include <cstddef>

#include "astcenc.h"

namespace {

// [0] is the size, [1] the alignment, and the rest are the field offsets in
// declaration order. lib.rs builds the same list with core::mem::offset_of! and
// compares them element by element.
const size_t LAYOUT[] = {
	sizeof(astcenc_config),
	alignof(astcenc_config),
	offsetof(astcenc_config, profile),
	offsetof(astcenc_config, flags),
	offsetof(astcenc_config, block_x),
	offsetof(astcenc_config, block_y),
	offsetof(astcenc_config, block_z),
	offsetof(astcenc_config, cw_r_weight),
	offsetof(astcenc_config, cw_g_weight),
	offsetof(astcenc_config, cw_b_weight),
	offsetof(astcenc_config, cw_a_weight),
	offsetof(astcenc_config, a_scale_radius),
	offsetof(astcenc_config, rgbm_m_scale),
	offsetof(astcenc_config, tune_partition_count_limit),
	offsetof(astcenc_config, tune_2partition_index_limit),
	offsetof(astcenc_config, tune_3partition_index_limit),
	offsetof(astcenc_config, tune_4partition_index_limit),
	offsetof(astcenc_config, tune_block_mode_limit),
	offsetof(astcenc_config, tune_refinement_limit),
	offsetof(astcenc_config, tune_candidate_limit),
	offsetof(astcenc_config, tune_2partitioning_candidate_limit),
	offsetof(astcenc_config, tune_3partitioning_candidate_limit),
	offsetof(astcenc_config, tune_4partitioning_candidate_limit),
	offsetof(astcenc_config, tune_db_limit),
	offsetof(astcenc_config, tune_mse_overshoot),
	offsetof(astcenc_config, tune_2partition_early_out_limit_factor),
	offsetof(astcenc_config, tune_3partition_early_out_limit_factor),
	offsetof(astcenc_config, tune_2plane_early_out_limit_correlation),
	offsetof(astcenc_config, tune_search_mode0_enable),
	offsetof(astcenc_config, progress_callback),
};

} // namespace

extern "C" const size_t* dashpack_astcenc_config_layout(size_t* count)
{
	*count = sizeof(LAYOUT) / sizeof(LAYOUT[0]);
	return LAYOUT;
}
