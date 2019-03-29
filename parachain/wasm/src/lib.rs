///////////////////////////////////////////////////////////////////////////////
//
//  Copyright 2018-2019 Airalab <research@aira.life> 
//
//  Licensed under the Apache License, Version 2.0 (the "License");
//  you may not use this file except in compliance with the License.
//  You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
//
///////////////////////////////////////////////////////////////////////////////
//! WASM validation for robonomics parachain.

#![no_std]

#![feature(
	alloc, core_intrinsics, lang_items, core_panic_info, alloc_error_handler
)]

extern crate alloc;
extern crate wee_alloc;
extern crate pwasm_libc;
extern crate robonomics_parachain as robonomics;
extern crate polkadot_parachain as parachain;
extern crate tiny_keccak;

// Define global allocator.
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use core::{intrinsics, panic};
use parachain::ValidationResult;
use parachain::codec::{Encode, Decode};
use robonomics::{HeadData, BlockData};

#[panic_handler]
#[no_mangle]
pub fn panic(_info: &panic::PanicInfo) -> ! {
	unsafe {
		intrinsics::abort()
	}
}

#[alloc_error_handler]
#[no_mangle]
pub fn oom(_: ::core::alloc::Layout) -> ! {
	unsafe {
		intrinsics::abort();
	}
}

#[no_mangle]
pub extern fn validate(offset: usize, len: usize) -> usize {
	let params = unsafe { ::parachain::load_params(offset, len) };
	let parent_head = HeadData::decode(&mut &params.parent_head[..])
		.expect("invalid parent head format.");

	let block_data = BlockData::decode(&mut &params.block_data[..])
		.expect("invalid block data format.");

	let parent_hash = ::tiny_keccak::keccak256(&params.parent_head[..]);

	match ::robonomics::execute(parent_hash, parent_head, &block_data) {
		Ok(new_head) => parachain::write_result(ValidationResult { head_data: new_head.encode() }),
		Err(_) => panic!("execution failure"),
	}
}
