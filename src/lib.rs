// Copyright (C) 2023 Matthew Waters <matthew@centricular.com>
//
// Licensed under the MIT license <LICENSE-MIT> or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

#![deny(missing_debug_implementations)]
#![deny(missing_docs)]

//! # cdp-types
//!
//! Provides the necessary infrastructure to read and write CDP (Caption Distribution Packet)
//!
//! The reference for this implementation is the `SMPTE 334-2-2007` specification.

pub use cea708_types;

mod parser;
mod svc;
mod writer;

#[macro_use]
extern crate log;

/// Various possible errors when parsing data
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParserError {
    /// The length of the data does not match the length in the data
    #[error("The length of the data ({actual}) does not match the advertised expected ({expected}) length")]
    LengthMismatch {
        /// Expected minimum size of the data
        expected: usize,
        /// The actual length of the data
        actual: usize,
    },
    /// Some magic byte/s do not have the correct value
    #[error("Some magic byte/s do not have the correct value")]
    WrongMagic,
    /// Unrecognied framerate value
    #[error("The framerate specified is not known by this implementation")]
    UnknownFramerate,
    /// Some 'fixed' bits did not have the correct value
    #[error("Some fixed bits did not have the correct value")]
    InvalidFixedBits,
    /// CEA-608 bytes were found after CEA-708 bytes
    #[error("CEA-608 compatibility bytes were found after CEA-708 bytes")]
    Cea608AfterCea708,
    /// Failed to validate the checksum
    #[error("The computed checksum value does not match the stored checksum value")]
    ChecksumFailed,
    /// Sequence count differs between the header and the footer.  Usually indicates this packet was
    /// spliced together incorrectly.
    #[error("The sequence count differs between the header and the footer")]
    SequenceCountMismatch,
    /// The service information contains conflicting service numbers.
    #[error("The service descriptor has different values")]
    ServiceNumberMismatch,
    /// The service number is not valid.
    #[error("The service number is not valid")]
    InvalidServiceNumber,
    /// The service descriptor contains a different set of flags to the CDP.
    #[error("The service descriptor contains a different set of flags to the CDP")]
    ServiceFlagsMismatched,
}

impl From<cea708_types::ParserError> for ParserError {
    fn from(value: cea708_types::ParserError) -> Self {
        match value {
            cea708_types::ParserError::Cea608AfterCea708 { byte_pos: _ } => {
                ParserError::Cea608AfterCea708
            }
            cea708_types::ParserError::LengthMismatch { expected, actual } => {
                ParserError::LengthMismatch { expected, actual }
            }
        }
    }
}

pub use cea708_types::WriterError;

static FRAMERATES: [Framerate; 8] = [
    Framerate {
        id: 0x1,
        numer: 24000,
        denom: 1001,
    },
    Framerate {
        id: 0x2,
        numer: 24,
        denom: 1,
    },
    Framerate {
        id: 0x3,
        numer: 25,
        denom: 1,
    },
    Framerate {
        id: 0x4,
        numer: 30000,
        denom: 1001,
    },
    Framerate {
        id: 0x5,
        numer: 30,
        denom: 1,
    },
    Framerate {
        id: 0x6,
        numer: 50,
        denom: 1,
    },
    Framerate {
        id: 0x7,
        numer: 60000,
        denom: 1001,
    },
    Framerate {
        id: 0x8,
        numer: 60,
        denom: 1,
    },
];

/// A framerate as found in a CDP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Framerate {
    id: u8,
    numer: u32,
    denom: u32,
}

/// A CDP framerate.
impl Framerate {
    /// Create a [`Framerate`] from an identifier as found in a CDP.
    ///
    /// # Examples
    ///
    /// ```
    /// # use cdp_types::Framerate;
    /// let frame = Framerate::from_id(0x8).unwrap();
    /// assert_eq!(frame.id(), 0x8);
    /// assert_eq!(frame.numer(), 60);
    /// assert_eq!(frame.denom(), 1);
    ///
    /// assert!(Framerate::from_id(0x0).is_none());
    /// ```
    pub fn from_id(id: u8) -> Option<Framerate> {
        FRAMERATES.iter().find(|f| f.id == id).copied()
    }

    /// The identifier for this [`Framerate`] in a CDP.
    pub fn id(&self) -> u8 {
        self.id
    }

    /// The numerator component of this [`Framerate`]
    pub fn numer(&self) -> u32 {
        self.numer
    }

    /// The denominator component of this [`Framerate`]
    pub fn denom(&self) -> u32 {
        self.denom
    }
}

/// A set of flags available in a CDP.
pub(crate) struct Flags {
    time_code: bool,
    cc_data: bool,
    svc_info: bool,
    svc_info_start: bool,
    svc_info_change: bool,
    svc_info_complete: bool,
    caption_service_active: bool,
    _reserved: bool,
}

impl Flags {
    const TIME_CODE_PRESENT: u8 = 0x80;
    const CC_DATA_PRESENT: u8 = 0x40;
    const SVC_INFO_PRESENT: u8 = 0x20;
    const SVC_INFO_START: u8 = 0x10;
    const SVC_INFO_CHANGE: u8 = 0x08;
    const SVC_INFO_COMPLETE: u8 = 0x04;
    const CAPTION_SERVICE_ACTIVE: u8 = 0x02;
}

impl From<u8> for Flags {
    fn from(value: u8) -> Self {
        Self {
            time_code: (value & Self::TIME_CODE_PRESENT) > 0,
            cc_data: (value & Self::CC_DATA_PRESENT) > 0,
            svc_info: (value & Self::SVC_INFO_PRESENT) > 0,
            svc_info_start: (value & Self::SVC_INFO_START) > 0,
            svc_info_change: (value & Self::SVC_INFO_CHANGE) > 0,
            svc_info_complete: (value & Self::SVC_INFO_COMPLETE) > 0,
            caption_service_active: (value & Self::CAPTION_SERVICE_ACTIVE) > 0,
            _reserved: (value & 0x01) > 0,
        }
    }
}

impl From<Flags> for u8 {
    fn from(value: Flags) -> Self {
        let mut ret = 0x1;
        if value.time_code {
            ret |= Flags::TIME_CODE_PRESENT;
        }
        if value.cc_data {
            ret |= Flags::CC_DATA_PRESENT;
        }
        if value.svc_info {
            ret |= Flags::SVC_INFO_PRESENT;
        }
        if value.svc_info_start {
            ret |= Flags::SVC_INFO_START;
        }
        if value.svc_info_change {
            ret |= Flags::SVC_INFO_CHANGE;
        }
        if value.svc_info_complete {
            ret |= Flags::SVC_INFO_COMPLETE;
        }
        if value.caption_service_active {
            ret |= Flags::CAPTION_SERVICE_ACTIVE;
        }
        ret
    }
}

/// A time code as available in a CDP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeCode {
    hours: u8,
    minutes: u8,
    seconds: u8,
    frames: u8,
    field: bool,
    drop_frame: bool,
}

impl TimeCode {
    /// Construct a new [`TimeCode`] value.
    ///
    /// # Examples
    ///
    /// ```
    /// # use cdp_types::TimeCode;
    /// let tc = TimeCode::new(1, 2, 3, 4, true, false);
    /// assert_eq!(tc.hours(), 1);
    /// assert_eq!(tc.minutes(), 2);
    /// assert_eq!(tc.seconds(), 3);
    /// assert_eq!(tc.frames(), 4);
    /// assert!(tc.field());
    /// assert!(!tc.drop_frame());
    /// ```
    pub fn new(
        hours: u8,
        minutes: u8,
        seconds: u8,
        frames: u8,
        field: bool,
        drop_frame: bool,
    ) -> Self {
        Self {
            hours,
            minutes,
            seconds,
            frames,
            field,
            drop_frame,
        }
    }

    /// The hour value of this [`TimeCode`].
    pub fn hours(&self) -> u8 {
        self.hours
    }

    /// The minute value of this [`TimeCode`].
    pub fn minutes(&self) -> u8 {
        self.minutes
    }

    /// The second value of this [`TimeCode`].
    pub fn seconds(&self) -> u8 {
        self.seconds
    }

    /// The frame value of this [`TimeCode`].
    pub fn frames(&self) -> u8 {
        self.frames
    }

    /// The field value of this [`TimeCode`].
    pub fn field(&self) -> bool {
        self.field
    }

    /// The drop frame value of this [`TimeCode`].
    pub fn drop_frame(&self) -> bool {
        self.drop_frame
    }
}

pub use parser::CDPParser;
pub use svc::{DigitalServiceEntry, FieldOrService, ServiceEntry, ServiceInfo};
pub use writer::CDPWriter;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use cea708_types::{tables, Cea608};
    use std::sync::OnceLock;

    #[derive(Debug)]
    pub(crate) struct ServiceData<'a> {
        pub service_no: u8,
        pub codes: &'a [tables::Code],
    }

    #[derive(Debug)]
    pub(crate) struct CCPacketData<'a> {
        pub sequence_no: u8,
        pub services: &'a [ServiceData<'a>],
    }

    #[derive(Debug)]
    pub(crate) struct CDPPacketData<'a> {
        pub data: &'a [u8],
        pub sequence_count: u16,
        pub time_code: Option<TimeCode>,
        pub packets: &'a [CCPacketData<'a>],
        pub cea608: &'a [Cea608],
    }

    #[derive(Debug)]
    pub(crate) struct TestCCData<'a> {
        pub framerate: Framerate,
        pub cdp_data: &'a [CDPPacketData<'a>],
    }

    pub fn test_init_log() {
        static TRACING: OnceLock<()> = OnceLock::new();
        TRACING.get_or_init(|| {
            env_logger::init();
        });
    }
}
