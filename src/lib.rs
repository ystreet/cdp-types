// Copyright (C) 2023 Matthew Waters <matthew@centricular.com>
//
// Licensed under the MIT license <LICENSE-MIT> or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! # cdp-types
//!
//! Provides the necessary infrastructure to read and write CDP (Caption Distribution Packet)
//!
//! The reference for this implementation is the `SMPTE 334-2-2007` specification.

pub use cea708_types;

mod svc;

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
struct Flags {
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
    field: u8,
    drop_frame: bool,
}

/// Parses CDP packets.
#[derive(Debug, Default)]
pub struct CDPParser {
    cc_data_parser: cea708_types::CCDataParser,
    time_code: Option<TimeCode>,
    framerate: Option<Framerate>,
    service_info: Option<ServiceInfo>,
    sequence: u16,
}

impl CDPParser {
    const MIN_PACKET_LEN: usize = 11;
    const TIME_CODE_ID: u8 = 0x71;
    const CC_DATA_ID: u8 = 0x72;
    const SVC_INFO_ID: u8 = 0x73;
    const CDP_FOOTER_ID: u8 = 0x74;

    /// Create a new [CDPParser]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a complete `CDP` packet into the parser for processing.
    pub fn parse(&mut self, data: &[u8]) -> Result<(), ParserError> {
        self.time_code = None;
        self.framerate = None;
        self.sequence = 0;

        trace!("parsing {data:?}");

        if data.len() < Self::MIN_PACKET_LEN {
            return Err(ParserError::LengthMismatch {
                expected: Self::MIN_PACKET_LEN,
                actual: data.len(),
            });
        }

        if (data[0], data[1]) != (0x96, 0x69) {
            return Err(ParserError::WrongMagic);
        }

        let len = data[2] as usize;
        if data.len() != len {
            return Err(ParserError::LengthMismatch {
                expected: len,
                actual: data.len(),
            });
        }

        let framerate =
            Framerate::from_id((data[3] & 0xf0) >> 4).ok_or(ParserError::UnknownFramerate)?;

        let flags: Flags = data[4].into();

        let sequence_count = (data[5] as u16) << 8 | data[6] as u16;

        let mut idx = 7;
        let time_code = if flags.time_code {
            trace!("attempting to parse time code");
            if data.len() < idx + 5 {
                return Err(ParserError::LengthMismatch {
                    expected: idx + 5,
                    actual: data.len(),
                });
            }
            if data[idx] != Self::TIME_CODE_ID {
                return Err(ParserError::WrongMagic);
            }

            idx += 1;
            if (data[idx] & 0xc0) != 0xc0 {
                return Err(ParserError::InvalidFixedBits);
            }
            let hours = ((data[idx] & 0x30) >> 4) * 10 + (data[idx] & 0x0f);

            idx += 1;
            if (data[idx] & 0x80) != 0x80 {
                return Err(ParserError::InvalidFixedBits);
            }
            let minutes = ((data[idx] & 0x70) >> 4) * 10 + (data[idx] & 0x0f);

            idx += 1;
            let field = (data[idx] & 0x80) >> 7;
            let seconds = ((data[idx] & 0x70) >> 4) * 10 + (data[idx] & 0x0f);

            idx += 1;
            let drop_frame = (data[idx] & 0x80) > 0;
            if (data[idx] & 0x40) != 0x00 {
                return Err(ParserError::InvalidFixedBits);
            }
            let frames = ((data[idx] & 0x30) >> 4) * 10 + (data[idx] & 0x0f);

            idx += 1;
            Some(TimeCode {
                hours,
                minutes,
                seconds,
                frames,
                field,
                drop_frame,
            })
        } else {
            None
        };

        let cc_data = if flags.cc_data {
            trace!("attempting to parse cc_data");
            if data.len() < idx + 2 {
                return Err(ParserError::LengthMismatch {
                    expected: idx + 2,
                    actual: data.len(),
                });
            }
            if data[idx] != Self::CC_DATA_ID {
                return Err(ParserError::WrongMagic);
            }
            idx += 1;

            if (data[idx] & 0xe0) != 0xe0 {
                return Err(ParserError::InvalidFixedBits);
            }
            let cc_count = (data[idx] & 0x1f) as usize;
            idx += 1;
            if data.len() < idx + cc_count * 3 {
                return Err(ParserError::LengthMismatch {
                    expected: idx + cc_count * 3,
                    actual: data.len(),
                });
            }
            let mut cc_data = vec![0x80 | 0x40 | cc_count as u8, 0xFF];
            cc_data.extend_from_slice(&data[idx..idx + cc_count * 3]);
            idx += cc_count * 3;
            Some(cc_data)
        } else {
            None
        };

        let service_info = if flags.svc_info {
            trace!("attempting to parse svc info");
            if data.len() < idx + 2 {
                return Err(ParserError::LengthMismatch {
                    expected: idx + 2,
                    actual: data.len(),
                });
            }
            if data[idx] != Self::SVC_INFO_ID {
                return Err(ParserError::WrongMagic);
            }
            let svc_count = (data[idx + 1] & 0x0f) as usize;
            let svc_size = 2 + 7 * svc_count;
            let service_info = ServiceInfo::parse(&data[idx..idx + svc_size])?;
            if service_info.is_start() != flags.svc_info_start {
                return Err(ParserError::ServiceFlagsMismatched);
            }
            if service_info.is_change() != flags.svc_info_change {
                return Err(ParserError::ServiceFlagsMismatched);
            }
            if service_info.is_complete() != flags.svc_info_complete {
                return Err(ParserError::ServiceFlagsMismatched);
            }
            idx += svc_size;
            Some(service_info)
        } else {
            None
        };

        if data.len() < idx + 2 {
            return Err(ParserError::LengthMismatch {
                expected: idx + 2,
                actual: data.len(),
            });
        }

        // future section handling
        while data[idx] != Self::CDP_FOOTER_ID {
            trace!("attempting to parse future section");
            if data[idx] < 0x75 || data[idx] > 0xEF {
                return Err(ParserError::WrongMagic);
            }
            idx += 1;
            let len = data[idx] as usize;
            if data.len() < idx + len {
                return Err(ParserError::LengthMismatch {
                    expected: idx + len,
                    actual: data.len(),
                });
            }
            idx += 1;
            // TODO: handle future_section
            idx += len;
            if data.len() < idx + 2 {
                return Err(ParserError::LengthMismatch {
                    expected: idx + 2,
                    actual: data.len(),
                });
            }
        }

        // handle cdp footer
        trace!("attempting to parse footer");
        if data.len() < idx + 4 {
            return Err(ParserError::LengthMismatch {
                expected: idx + 4,
                actual: data.len(),
            });
        }
        if data[idx] != Self::CDP_FOOTER_ID {
            return Err(ParserError::WrongMagic);
        }
        idx += 1;
        let footer_sequence_count = (data[idx] as u16) << 8 | data[idx + 1] as u16;
        if sequence_count != footer_sequence_count {
            return Err(ParserError::SequenceCountMismatch);
        }
        idx += 2;

        let mut checksum: u8 = 0;
        for d in data[..data.len() - 1].iter() {
            checksum = checksum.wrapping_add(*d);
        }
        // 256 - checksum without having to use a type larger than u8
        let checksum_byte = (!checksum).wrapping_add(1);
        trace!(
            "calculate checksum {checksum_byte:#x}, checksum in data {:#x}",
            data[idx]
        );
        if checksum_byte != data[idx] {
            return Err(ParserError::ChecksumFailed);
        }

        if let Some(cc_data) = cc_data {
            self.cc_data_parser.push(&cc_data)?;
        }
        self.framerate = Some(framerate);
        self.time_code = time_code;
        self.sequence = sequence_count;
        self.service_info = service_info;

        Ok(())
    }

    /// Clear any internal buffers
    pub fn flush(&mut self) {
        *self = Self::default();
    }

    /// The latest CDP time code that has been parsed
    pub fn time_code(&self) -> Option<TimeCode> {
        self.time_code
    }

    /// The latest CDP framerate that has been parsed
    pub fn framerate(&self) -> Option<Framerate> {
        self.framerate
    }

    /// The latest CDP sequence number that has been parsed
    pub fn sequence(&self) -> u16 {
        self.sequence
    }

    /// The latest Service Descriptor that has been parsed.
    pub fn service_info(&self) -> Option<&ServiceInfo> {
        self.service_info.as_ref()
    }

    /// Pop a valid [`cea708_types::DTVCCPacket`] or None if no packet could be parsed
    pub fn pop_packet(&mut self) -> Option<cea708_types::DTVCCPacket> {
        self.cc_data_parser.pop_packet()
    }

    /// Pop the list of [`cea708_types::Cea608`] contained in this packet
    pub fn cea608(&mut self) -> Option<&[cea708_types::Cea608]> {
        self.cc_data_parser.cea608()
    }
}

/// A struct for writing cc_data packets
#[derive(Debug)]
pub struct CDPWriter {
    cc_data: cea708_types::CCDataWriter,
    time_code: Option<TimeCode>,
    service_info: Option<ServiceInfo>,
    frame_rate: Framerate,
    sequence_count: u16,
}

impl CDPWriter {
    pub fn new(frame_rate: Framerate) -> Self {
        Self {
            cc_data: cea708_types::CCDataWriter::default(),
            time_code: None,
            service_info: None,
            frame_rate,
            sequence_count: 0,
        }
    }

    /// Push a [`cea708_types::DTVCCPacket`] for writing
    pub fn push_packet(&mut self, packet: cea708_types::DTVCCPacket) {
        self.cc_data.push_packet(packet)
    }

    /// Push a [`cea708_types::Cea608`] byte pair for writing
    pub fn push_cea608(&mut self, cea608: cea708_types::Cea608) {
        self.cc_data.push_cea608(cea608)
    }

    pub fn set_time_code(&mut self, time_code: Option<TimeCode>) {
        self.time_code = time_code;
    }

    pub fn set_service_info(&mut self, service_info: Option<ServiceInfo>) {
        self.service_info = service_info;
    }

    /// Set the next packet's sequence count to a specific value
    pub fn set_sequence_count(&mut self, sequence: u16) {
        self.sequence_count = sequence;
    }

    /// Clear all stored data
    pub fn flush(&mut self) {
        self.cc_data.flush();
        self.time_code = None;
        self.sequence_count = 0;
    }

    /// Write the next CDP packet taking the next relevant CEA-608 byte pairs and
    /// [`cea708_types::DTVCCPacket`]s.
    pub fn write<W: std::io::Write>(&mut self, w: &mut W) -> Result<(), std::io::Error> {
        let mut len = 7; // header
        if self.time_code.is_some() {
            len += 5;
        }
        let mut cc_data = Vec::new();
        self.cc_data.write(
            cea708_types::Framerate::new(self.frame_rate.numer(), self.frame_rate.denom()),
            &mut cc_data,
        )?;
        cc_data[1] = 0xe0 | (cc_data[0] & 0x1f);
        cc_data[0] = 0x72;
        len += cc_data.len();
        if let Some(service) = self.service_info.as_ref() {
            len += service.byte_len();
        }
        len += 4; // footer

        assert!(len <= u8::MAX as usize);

        let mut flags = Flags::CC_DATA_PRESENT | 0x1;
        if self.time_code.is_some() {
            flags |= Flags::TIME_CODE_PRESENT;
        }
        if let Some(svc) = self.service_info.as_ref() {
            flags |= Flags::SVC_INFO_PRESENT;
            if svc.is_start() {
                flags |= Flags::SVC_INFO_START;
            }
            if svc.is_change() {
                flags |= Flags::SVC_INFO_CHANGE;
            }
            if svc.is_complete() {
                flags |= Flags::SVC_INFO_COMPLETE;
            }
        }

        let mut checksum: u8 = 0;
        let data = [
            0x96,
            0x69,
            (len & 0xff) as u8,
            self.frame_rate.id << 4 | 0x0f,
            flags,
            ((self.sequence_count & 0xff00) >> 8) as u8,
            (self.sequence_count & 0xff) as u8,
        ];
        for v in data.iter() {
            checksum = checksum.wrapping_add(*v);
        }
        w.write_all(&data)?;

        if let Some(time_code) = self.time_code {
            let data = [
                0x71,
                0xc0 | ((time_code.hours / 10) << 4) | (time_code.hours % 10),
                0x80 | ((time_code.minutes / 10) << 4) | (time_code.minutes % 10),
                ((time_code.field & 0x1) << 7)
                    | ((time_code.seconds / 10) << 4)
                    | (time_code.seconds % 10),
                if time_code.drop_frame { 0x80 } else { 0x0 }
                    | ((time_code.frames / 10) << 4)
                    | (time_code.frames % 10),
            ];
            for v in data.iter() {
                checksum = checksum.wrapping_add(*v);
            }
            w.write_all(&data)?;
        }

        for v in cc_data.iter() {
            checksum = checksum.wrapping_add(*v);
        }
        w.write_all(&cc_data)?;

        let mut svc_data = vec![];
        if let Some(service) = self.service_info.as_mut() {
            service.write(&mut svc_data)?;
        }

        for v in svc_data.iter() {
            checksum = checksum.wrapping_add(*v);
        }
        w.write_all(&svc_data)?;

        let data = [
            0x74,
            ((self.sequence_count & 0xff00) >> 8) as u8,
            (self.sequence_count & 0xff) as u8,
        ];
        for v in data.iter() {
            checksum = checksum.wrapping_add(*v);
        }
        w.write_all(&data)?;
        // 256 - checksum without having to use a type larger than u8
        let checksum_byte = (!checksum).wrapping_add(1);
        debug_assert!(checksum_byte == ((256 - checksum as u16) as u8));
        w.write_all(&[checksum_byte])?;

        Ok(())
    }
}

pub use svc::{ServiceInfo, ServiceEntry, FieldOrService, DigitalServiceEntry};

#[cfg(test)]
mod test {
    use super::*;
    use crate::tests::*;
    use cea708_types::{tables, Cea608, DTVCCPacket, Service};

    #[derive(Debug)]
    struct ServiceData<'a> {
        service_no: u8,
        codes: &'a [tables::Code],
    }

    #[derive(Debug)]
    struct CCPacketData<'a> {
        sequence_no: u8,
        services: &'a [ServiceData<'a>],
    }

    #[derive(Debug)]
    struct CDPPacketData<'a> {
        data: &'a [u8],
        sequence_count: u16,
        time_code: Option<TimeCode>,
        packets: &'a [CCPacketData<'a>],
        cea608: &'a [Cea608],
    }

    #[derive(Debug)]
    struct TestCCData<'a> {
        framerate: Framerate,
        cdp_data: &'a [CDPPacketData<'a>],
    }

    static PARSE_CDP: [TestCCData; 4] = [
        // simple packet with cc_data and a time code
        TestCCData {
            framerate: FRAMERATES[2],
            cdp_data: &[CDPPacketData {
                data: &[
                    0x96, // magic
                    0x69,
                    0x18,               // cdp_len
                    0x3f,               // framerate
                    0x80 | 0x40 | 0x01, // flags
                    0x12,               // sequence counter
                    0x34,
                    0x71,        // time code id
                    0xc0 | 0x17, // hours
                    0x80 | 0x59, // minutes
                    0x80 | 0x57, // seconds
                    0x80 | 0x18, // frames
                    0x72,        // cc_data id
                    0xe0 | 0x02, // cc_count
                    0xFF,
                    0x02,
                    0x21,
                    0xFE,
                    0x41,
                    0x00,
                    0x74, // cdp footer
                    0x12,
                    0x34,
                    0xA4, // checksum
                ],
                sequence_count: 0x1234,
                time_code: Some(TimeCode {
                    hours: 17,
                    minutes: 59,
                    seconds: 57,
                    frames: 18,
                    field: 1,
                    drop_frame: true,
                }),
                packets: &[CCPacketData {
                    sequence_no: 0,
                    services: &[ServiceData {
                        service_no: 1,
                        codes: &[tables::Code::LatinCapitalA],
                    }],
                }],
                cea608: &[],
            }],
        },
        // simple packet with no time code
        TestCCData {
            framerate: FRAMERATES[2],
            cdp_data: &[CDPPacketData {
                data: &[
                    0x96, // magic
                    0x69,
                    0x13,        // cdp_len
                    0x3f,        // framerate
                    0x40 | 0x01, // flags
                    0x12,        // sequence counter
                    0x34,
                    0x72,        // cc_data id
                    0xe0 | 0x02, // cc_count
                    0xFF,
                    0x02,
                    0x21,
                    0xFE,
                    0x41,
                    0x00,
                    0x74, // cdp footer
                    0x12,
                    0x34,
                    0xB9, // checksum
                ],
                sequence_count: 0x1234,
                time_code: None,
                packets: &[CCPacketData {
                    sequence_no: 0,
                    services: &[ServiceData {
                        service_no: 1,
                        codes: &[tables::Code::LatinCapitalA],
                    }],
                }],
                cea608: &[],
            }],
        },
        // simple packet with svc_info (that is currently ignored)
        TestCCData {
            framerate: FRAMERATES[2],
            cdp_data: &[CDPPacketData {
                data: &[
                    0x96, // magic
                    0x69,
                    0x14,        // cdp_len
                    0x3f,        // framerate
                    0x20 | 0x10 | 0x04 | 0x01, // flags
                    0x12,        // sequence counter
                    0x34,
                    0x73, // svc_info id
                    0x80 | 0x40 | 0x10 | 0x01, // reserved | start | change | complete | count
                    0x80, // reserved | service number
                    b'e',
                    b'n',
                    b'g',
                    0x40 | 0x3e, // is_digital | reserved | field/service
                    0x3f, // reader | wide | reserved
                    0xff, // reserved
                    0x74, // cdp footer
                    0x12,
                    0x34,
                    0xbf, // checksum
                ],
                sequence_count: 0x1234,
                time_code: None,
                packets: &[],
                cea608: &[],
            }],
        },
        // simple packet with future section (that is currently ignored)
        TestCCData {
            framerate: FRAMERATES[2],
            cdp_data: &[CDPPacketData {
                data: &[
                    0x96, // magic
                    0x69, 0x0F, // cdp_len
                    0x3f, // framerate
                    0x01, // flags
                    0x12, // sequence counter
                    0x34, 0x75, // svc_info id
                    0x02, 0x45, 0x67, 0x74, // cdp footer
                    0x12, 0x34, 0x8F, // checksum
                ],
                sequence_count: 0x1234,
                time_code: None,
                packets: &[],
                cea608: &[],
            }],
        },
    ];

    #[test]
    fn cdp_parse() {
        test_init_log();
        for (i, test_data) in PARSE_CDP.iter().enumerate() {
            info!("parsing {i}: {test_data:?}");
            let mut parser = CDPParser::new();
            for cdp in test_data.cdp_data.iter() {
                parser.parse(cdp.data).unwrap();
                assert_eq!(parser.time_code(), cdp.time_code);
                assert_eq!(parser.sequence(), cdp.sequence_count);
                assert_eq!(parser.framerate(), Some(test_data.framerate));
                let mut expected_packet_iter = cdp.packets.iter();
                while let Some(packet) = parser.pop_packet() {
                    let expected = expected_packet_iter.next().unwrap();
                    assert_eq!(expected.sequence_no, packet.sequence_no());
                    let services = packet.services();
                    let mut expected_service_iter = expected.services.iter();
                    for parsed_service in services.iter() {
                        let expected_service = expected_service_iter.next().unwrap();
                        assert_eq!(parsed_service.number(), expected_service.service_no);
                        assert_eq!(expected_service.codes, parsed_service.codes());
                    }
                    assert!(expected_service_iter.next().is_none());
                }
                assert_eq!(parser.cea608().unwrap_or(&[]), cdp.cea608);
                assert!(expected_packet_iter.next().is_none());
            }
            assert!(parser.pop_packet().is_none());
        }
    }

    static WRITE_CDP: [TestCCData; 2] = [
        // simple packet with a single service and single code
        TestCCData {
            framerate: FRAMERATES[2],
            cdp_data: &[CDPPacketData {
                data: &[
                    0x96,
                    0x69,               // magic
                    0x18,               // cdp_len
                    0x3f,               //framerate
                    0x80 | 0x40 | 0x01, // flags
                    0x12,
                    0x34,        // sequence counter
                    0x71,        // time code id
                    0xc0 | 0x17, // hours
                    0x80 | 0x59, // minutes
                    0x80 | 0x57, // seconds
                    0x80 | 0x18, // frames
                    0x72,        // cc_data id
                    0xe0 | 0x02,
                    0xFF,
                    0x02,
                    0x21,
                    0xFE,
                    0x41,
                    0x00,
                    0x74, // footer
                    0x12,
                    0x34,
                    0xA4, //checksum
                ],
                sequence_count: 0x1234,
                time_code: Some(TimeCode {
                    hours: 17,
                    minutes: 59,
                    seconds: 57,
                    frames: 18,
                    field: 1,
                    drop_frame: true,
                }),
                packets: &[CCPacketData {
                    sequence_no: 0,
                    services: &[ServiceData {
                        service_no: 1,
                        codes: &[tables::Code::LatinCapitalA],
                    }],
                }],
                cea608: &[],
            }],
        },
        TestCCData {
            framerate: FRAMERATES[2],
            cdp_data: &[CDPPacketData {
                data: &[
                    0x96, // magic
                    0x69,
                    0x13,        // cdp_len
                    0x3f,        // framerate
                    0x40 | 0x01, // flags
                    0x34,        // sequence counter
                    0x12,
                    0x72,        // cc_data id
                    0xe0 | 0x02, // cc_count
                    0xFF,
                    0x02,
                    0x21,
                    0xFE,
                    0x41,
                    0x00,
                    0x74, // cdp footer
                    0x34,
                    0x12,
                    0xB9, // checksum
                ],
                sequence_count: 0x3412,
                time_code: None,
                packets: &[CCPacketData {
                    sequence_no: 0,
                    services: &[ServiceData {
                        service_no: 1,
                        codes: &[tables::Code::LatinCapitalA],
                    }],
                }],
                cea608: &[],
            }],
        },
    ];

    #[test]
    fn packet_write_cc_data() {
        test_init_log();
        for test_data in WRITE_CDP.iter() {
            info!("writing {test_data:?}");
            let mut writer = CDPWriter::new(test_data.framerate);
            for cdp_data in test_data.cdp_data.iter() {
                let mut packet_iter = cdp_data.packets.iter();
                if let Some(packet_data) = packet_iter.next() {
                    let mut pack = DTVCCPacket::new(packet_data.sequence_no);
                    for service_data in packet_data.services.iter() {
                        let mut service = Service::new(service_data.service_no);
                        for code in service_data.codes.iter() {
                            service.push_code(code).unwrap();
                        }
                        pack.push_service(service).unwrap();
                    }
                    writer.push_packet(pack);
                }
                for pair in cdp_data.cea608 {
                    writer.push_cea608(*pair);
                }
                writer.set_time_code(cdp_data.time_code);
                writer.set_sequence_count(cdp_data.sequence_count);
                let mut written = vec![];
                writer.write(&mut written).unwrap();
                assert_eq!(cdp_data.data, &written);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use once_cell::sync::Lazy;

    static TRACING: Lazy<()> = Lazy::new(env_logger::init);

    pub fn test_init_log() {
        Lazy::force(&TRACING);
    }
}
