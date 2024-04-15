// Copyright Cartesi and individual authors (see AUTHORS)
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::io::ErrorKind;

use libc::c_void;
use serde::{Deserialize, Serialize};
use validator::Validate;
use regex::Regex;
use lazy_static::lazy_static;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[derive(Clone)]
pub struct RollupFd(*mut cmt_rollup_t);

impl RollupFd {
    pub fn create() -> Result<Self, i32> {
        unsafe {
            let zeroed = Box::leak(Box::new(std::mem::zeroed::<cmt_rollup_t>()));
            let result = cmt_rollup_init(zeroed);
            if result != 0 {
                Err(result)
            } else {
                Ok(RollupFd(zeroed))
            }
        }
    }
}

impl Drop for RollupFd {
    fn drop(&mut self) {
        unsafe {
            cmt_rollup_fini(self.0);
            drop(Box::from_raw(self.0));
        }
    }
}

unsafe impl Sync for RollupFd {}
unsafe impl Send for RollupFd {}

pub const REQUEST_TYPE_ADVANCE_STATE: u32 = 0;
pub const REQUEST_TYPE_INSPECT_STATE: u32 = 1;
pub const CARTESI_ROLLUP_ADDRESS_SIZE: u32 = 20;

lazy_static! {
    static ref ETH_ADDR_REGEXP: Regex = Regex::new(r"0x[0-9a-fA-F]{1,42}$").unwrap();
    static ref ETH_U256_REGEXP: Regex = Regex::new(r"0x[0-9a-fA-F]{1,64}$").unwrap();
}

#[derive(Debug, Default)]
pub struct RollupError {
    message: String,
}

impl RollupError {
    pub fn new(message: &str) -> Self {
        RollupError {
            message: String::from(message),
        }
    }
}

impl std::fmt::Display for RollupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rollup error: {}", &self.message)
    }
}

impl std::error::Error for RollupError {}

#[derive(Debug, Default, Clone, PartialEq, Eq, Copy)]
pub struct RollupFinish {
    pub accept_previous_request: bool,
    pub next_request_type: i32,
    pub next_request_payload_length: usize,
}

impl From<RollupFinish> for cmt_rollup_finish_t {
    fn from(other: RollupFinish) -> Self {
        cmt_rollup_finish_t {
            next_request_type: other.next_request_type,
            accept_previous_request: other.accept_previous_request,
            next_request_payload_length: other.next_request_payload_length as u32,
        }
    }
}

impl From<&mut RollupFinish> for cmt_rollup_finish_t {
    fn from(other: &mut RollupFinish) -> Self {
        cmt_rollup_finish_t {
            next_request_type: other.next_request_type,
            accept_previous_request: other.accept_previous_request,
            next_request_payload_length: other.next_request_payload_length as u32,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceMetadata {
    pub msg_sender: String,
    pub input_index: u64,
    pub block_number: u64,
    pub block_timestamp: u64,
}

impl From<cmt_rollup_advance_t> for AdvanceMetadata {
    fn from(other: cmt_rollup_advance_t) -> Self {
        let mut address = "0x".to_string();
        address.push_str(&hex::encode(&other.msg_sender));
        AdvanceMetadata {
            input_index: other.index,
            block_timestamp: other.block_timestamp,
            block_number: other.block_number,
            msg_sender: address,
        }
    }
}

impl From<cmt_rollup_finish_t> for RollupFinish {
    fn from(other: cmt_rollup_finish_t) -> Self {
        RollupFinish {
            next_request_type: other.next_request_type,
            accept_previous_request: other.accept_previous_request,
            next_request_payload_length: other.next_request_payload_length as usize,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceRequest {
    pub metadata: AdvanceMetadata,
    pub payload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectRequest {
    pub payload: String,
}

pub enum RollupRequest {
    Inspect(InspectRequest),
    Advance(AdvanceRequest),
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct FinishRequest {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectReport {
    pub reports: Vec<Report>
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Notice {
    pub payload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Voucher {
    #[validate(regex = "ETH_ADDR_REGEXP")]
    pub destination: String,
    #[validate(regex = "ETH_U256_REGEXP")]
    pub value: String,
    pub payload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Report {
    pub payload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Exception {
    pub payload: String,
}

pub enum RollupResponse {
    Finish(bool),
}

pub fn rollup_finish_request(
    fd: &RollupFd,
    finish: &mut RollupFinish,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut finish_c = Box::new(cmt_rollup_finish_t::from(&mut *finish));

    log::debug!("writing rollup finish request, yielding");

    let res = unsafe { cmt_rollup_finish(fd.0, finish_c.as_mut()) };

    if res < 0 {
        log::error!("failed to write finish request, IOCTL error {}", res);
        return Err(Box::new(RollupError::new(&format!(
            "IOCTL_ROLLUP_FINISH returned error {}",
            res
        ))));
    }

    *finish = RollupFinish::from(*finish_c);

    log::debug!("finish request written to rollup device: {:#?}", &finish);
    Ok(())
}

pub fn rollup_read_advance_state_request(
    fd: &RollupFd,
) -> Result<AdvanceRequest, Box<dyn std::error::Error>> {

    let mut advance_request = Box::new(cmt_rollup_advance_t {
        chain_id: 0,
        msg_sender: Default::default(),
        app_contract: Default::default(),
        block_number: 0,
        block_timestamp: 0,
        index: 0,
        payload_length: 0,
        payload: std::ptr::null::<::std::os::raw::c_uchar>() as *mut c_void,
    });

    let res = unsafe {
        cmt_rollup_read_advance_state(fd.0, advance_request.as_mut())
    };

    if res != 0 {
        return Err(Box::new(RollupError::new(&format!(
            "IOCTL_ROLLUP_READ_ADVANCE_STATE returned error {}",
            res
        ))));
    }

    if advance_request.payload_length == 0 {
        log::info!("read zero size payload from advance state request");
    }

    let mut payload: Vec<u8> = Vec::with_capacity(advance_request.payload_length as usize);
    if advance_request.payload_length > 0 {
        unsafe {
            std::ptr::copy(advance_request.payload, payload.as_mut_ptr() as *mut c_void, advance_request.payload_length as usize);
            payload.set_len(advance_request.payload_length as usize);
        }
    }

    let result = AdvanceRequest {
        metadata: AdvanceMetadata::from(*advance_request),
        payload: "0x".to_string() + &hex::encode(&payload),
    };

    Ok(result)
}


pub fn rollup_read_inspect_state_request(
    fd: &RollupFd,
) -> Result<InspectRequest, Box<dyn std::error::Error>> {

    let mut inspect_request = Box::new(cmt_rollup_inspect_t {
        payload_length: 0,
        payload: std::ptr::null::<::std::os::raw::c_uchar>() as *mut c_void,
    });

    let res = unsafe {
        cmt_rollup_read_inspect_state(fd.0, inspect_request.as_mut())
    };

    if res != 0 {
        return Err(Box::new(RollupError::new(&format!(
            "IOCTL_ROLLUP_READ_INSPECT_STATE returned error {}",
            res
        ))));
    }

    if inspect_request.payload_length == 0 {
        log::info!("read zero size payload from inspect state request");
    }

    println!("inspect_request.payload_length: {}", inspect_request.payload_length);
    let mut payload: Vec<u8> = Vec::with_capacity(inspect_request.payload_length as usize);
    if inspect_request.payload_length > 0 {
        unsafe {
            std::ptr::copy(inspect_request.payload, payload.as_mut_ptr() as *mut c_void, inspect_request.payload_length as usize);
            payload.set_len(inspect_request.payload_length as usize);
        }
    }

    let result = InspectRequest {
        payload: "0x".to_string() + &hex::encode(&payload),
    };

    Ok(result)
}

pub fn rollup_write_notice(
    fd: &RollupFd,
    notice: &mut Notice,
) -> Result<u64, Box<dyn std::error::Error>> {
    print_notice(notice);

    let binary_payload = match hex::decode(&notice.payload[2..]) {
        Ok(payload) => payload,
        Err(_err) => {
            return Err(Box::new(RollupError::new(&format!(
                "Error decoding notice payload, payload must be in Ethereum hex binary format"
            ))));
        }
    };

    let mut buffer: Vec<u8> = Vec::with_capacity(binary_payload.len());
    let length = binary_payload.len() as u64;
    let mut notice_index: std::os::raw::c_ulong = 0;

    let res = unsafe {
        std::ptr::copy(
            binary_payload.as_ptr(),
            buffer.as_mut_ptr(),
            binary_payload.len(),
        );

        cmt_rollup_emit_notice(fd.0, length as u32, buffer.as_mut_ptr() as *mut c_void, &mut notice_index)
    };

    if res != 0 {
        return Err(Box::new(RollupError::new(&format!(
            "IOCTL_ROLLUP_WRITE_NOTICE returned error {}",
            res
        ))));
    } else {
        log::debug!("notice with id {} successfully written!", notice_index);
    }

    Ok(notice_index as u64)
}


pub fn rollup_write_voucher(
    fd: &RollupFd,
    voucher: &mut Voucher,
) -> Result<u64, Box<dyn std::error::Error>> {
    print_voucher(voucher);

    let binary_payload = match hex::decode(&voucher.payload[2..]) {
        Ok(payload) => payload,
        Err(_err) => {
            return Err(Box::new(RollupError::new(&format!(
                "Error decoding voucher payload, it must be in Ethereum hex binary format"
            ))));
        }
    };
    let mut payload_buffer: Vec<u8> = Vec::with_capacity(binary_payload.len());
    let payload_data = payload_buffer.as_mut_ptr();
    let payload_length = binary_payload.len();

    let binary_value = match hex::decode(&voucher.value[2..]) {
        Ok(data) => data,
        Err(_err) => {
            return Err(Box::new(RollupError::new(&format!(
                "Error decoding voucher value, it must be in Ethereum hex binary format"
            ))));
        }
    };
    let mut value_buffer: Vec<u8> = Vec::with_capacity(binary_value.len());
    let value_data = value_buffer.as_mut_ptr();
    let value_length = binary_value.len();

    let address_c = match hex::decode(&voucher.destination[2..]) {
        Ok(res) => res,
        Err(e) => {
            return Err(Box::new(RollupError::new(&format!(
                "address not valid: {}",
                e
            ))));
        }
    };

    let mut voucher_index: std::os::raw::c_ulong = 0;
    let res = unsafe {
        std::ptr::copy(
            binary_payload.as_ptr(),
            payload_buffer.as_mut_ptr(),
            binary_payload.len(),
        );
        std::ptr::copy(
            binary_value.as_ptr(),
            value_buffer.as_mut_ptr(),
            binary_value.len(),
        );

        cmt_rollup_emit_voucher(
            fd.0,
            address_c.len() as u32,
            address_c.as_ptr() as *const c_void,
            value_length as u32,
            value_data as *mut c_void,
            payload_length as u32,
            payload_data as *mut c_void,
            &mut voucher_index,
        )
    };

    if res != 0 {
        return Err(Box::new(RollupError::new(&format!(
            "IOCTL_ROLLUP_WRITE_VOUCHER returned error {}",
            res
        ))));
    } else {
        log::debug!("voucher with id {} successfully written!", voucher_index);
    }

    Ok(voucher_index as u64)
}

pub fn rollup_write_report(fd: &RollupFd, report: &Report) -> Result<(), Box<dyn std::error::Error>> {
    print_report(report);

    let binary_payload = match hex::decode(&report.payload[2..]) {
        Ok(payload) => payload,
        Err(_err) => {
            return Err(Box::new(RollupError::new(&format!(
                "Error decoding report payload, payload must be in Ethereum hex binary format"
            ))));
        }
    };

    let mut buffer: Vec<u8> = Vec::with_capacity(binary_payload.len());

    let data = buffer.as_mut_ptr() as *mut c_void;
    let length = binary_payload.len();

    let res = unsafe {
        std::ptr::copy(
            binary_payload.as_ptr(),
            buffer.as_mut_ptr(),
            binary_payload.len(),
        );
        cmt_rollup_emit_report(fd.0, length as u32, data)
    };

    if res != 0 {
        return Err(Box::new(RollupError::new(&format!(
            "IOCTL_ROLLUP_WRITE_REPORT returned error {}",
            res
        ))));
    } else {
        log::debug!("report successfully written!");
    }

    Ok(())
}

pub fn rollup_throw_exception(
    fd: &RollupFd,
    exception: &Exception,
) -> Result<(), Box<dyn std::error::Error>> {
    print_exception(exception);

    let binary_payload = match hex::decode(&exception.payload[2..]) {
        Ok(payload) => payload,
        Err(_err) => {
            return Err(Box::new(RollupError::new(&format!(
                "Error decoding report payload, payload must be in Ethereum hex binary format"
            ))));
        }
    };

    let mut buffer: Vec<u8> = Vec::with_capacity(binary_payload.len());
    let length = binary_payload.len();
    let data = buffer.as_mut_ptr() as *mut c_void;

    let res = unsafe {
        std::ptr::copy(
            binary_payload.as_ptr(),
            buffer.as_mut_ptr(),
            binary_payload.len(),
        );
        cmt_rollup_emit_exception(fd.0, length as u32, data)
    };
    if res != 0 {
        return Err(Box::new(RollupError::new(&format!(
            "IOCTL_ROLLUP_THROW_EXCEPTION returned error {}",
            res
        ))));
    } else {
        log::debug!("exception successfully thrown!");
    }
    Ok(())
}

pub async fn perform_rollup_finish_request(
    fd: &RollupFd,
    accept: bool,
) -> std::io::Result<RollupFinish> {
    let mut finish_request = RollupFinish::default();
    finish_request.accept_previous_request = accept;

    match rollup_finish_request(fd, &mut finish_request) {
        Ok(_) => {
            dbg!(&finish_request);
            Ok(finish_request)
        }
        Err(e) => {
            log::error!("error inserting finish request, details: {}", e.to_string());
            Err(std::io::Error::new(ErrorKind::Other, e.to_string()))
        }
    }
}

/// Read advance/inspect request from rollup device
pub async fn handle_rollup_requests(
    fd: &RollupFd,
    finish_request: RollupFinish,
) -> Result<RollupRequest, std::io::Error> {
    let next_request_type = finish_request.next_request_type as u32;

    match next_request_type {
        REQUEST_TYPE_ADVANCE_STATE => {
            log::debug!("handle advance state request...");
            let advance_request = {
                // Read advance request from rollup device
                match rollup_read_advance_state_request(fd) {
                    Ok(r) => r,
                    Err(e) => {
                        return Err(std::io::Error::new(ErrorKind::Other, e.to_string()));
                    }
                }
            };
            if log::log_enabled!(log::Level::Info) {
                print_advance(&advance_request);
            }
            // Send newly read advance request to http service
            Ok(RollupRequest::Advance(advance_request))
        }
        REQUEST_TYPE_INSPECT_STATE => {
            log::debug!("handle inspect state request...");
            // Read inspect request from rollup device
            let inspect_request = {
                match rollup_read_inspect_state_request(fd) {
                    Ok(r) => r,
                    Err(e) => {
                        return Err(std::io::Error::new(ErrorKind::Other, e.to_string()));
                    }
                }
            };
            if log::log_enabled!(log::Level::Info) {
                print_inspect(&inspect_request);
            }
            // Send newly read inspect request to http service
            Ok(RollupRequest::Inspect(inspect_request))
        }
        _ => {
            return Err(std::io::Error::new(
                ErrorKind::Unsupported,
                "request type unsupported",
            ));
        }
    }
}

pub fn format_address_printout(address: &str, printout_address: &mut String) {
    if address.starts_with("0x") {
        printout_address.push_str(address);
    } else {
        printout_address.push_str(&format!("0x{}", address));
    }
}

pub fn print_advance(advance: &AdvanceRequest) {
    let mut advance_request_printout = String::new();
    advance_request_printout.push_str("advance: {{msg_sender: ");
    format_address_printout(&advance.metadata.msg_sender, &mut advance_request_printout);
    advance_request_printout.push_str(&format!(
        " block_number: {} block_timestamp: {} input_index: {} }}",
        advance.metadata.block_number,
        advance.metadata.block_timestamp,
        advance.metadata.input_index
    ));
    log::debug!("{}", &advance_request_printout);
}

pub fn print_inspect(inspect: &InspectRequest) {
    log::debug!(
        "Inspect: {{ length: {} payload: {}}}",
        inspect.payload.len(),
        inspect.payload
    );
}

pub fn print_notice(notice: &Notice) {
    log::debug!(
        "Notice: {{ length: {} payload: {}}}",
        notice.payload.len(),
        notice.payload
    );
}


pub fn print_voucher(voucher: &Voucher) {
    let mut voucher_request_printout = String::new();
    voucher_request_printout.push_str("voucher: {{ destination: ");
    format_address_printout(&voucher.destination, &mut voucher_request_printout);
    voucher_request_printout.push_str(&format!(
        " length: {} payload: {} }}",
        voucher.payload.len(),
        voucher.payload
    ));
    log::debug!("{}", &voucher_request_printout);
}

pub fn print_report(report: &Report) {
    log::debug!(
        "report: {{ length: {} payload: {}}}",
        report.payload.len(),
        report.payload
    );
}

pub fn print_exception(exception: &Exception) {
    log::debug!(
        "exception: {{ length: {} payload: {}}}",
        exception.payload.len(),
        exception.payload
    );
}
