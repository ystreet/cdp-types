// Copyright (C) 2023 Matthew Waters <matthew@centricular.com>
//
// Licensed under the MIT license <LICENSE-MIT> or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use crate::{Flags, Framerate, ParserError, ServiceInfo, TimeCode};

/// Parses CDP packets.
///
/// # Examples
///
/// ```
/// # use cdp_types::*;
/// # use cdp_types::cea708_types::Cea608;
/// let mut parser = CDPParser::new();
/// let data = [
///     0x96, 0x69,                // magic
///     0x27,                      // cdp_len
///     0x3f,                      // framerate
///     0x80 | 0x40 | 0x20 | 0x10 | 0x04 | 0x02 | 0x01, // flags
///     0x12, 0x34,                // sequence counter
///     0x71,                      // time code id
///     0xc0 | 0x17,               // hours
///     0x80 | 0x59,               // minutes
///     0x80 | 0x57,               // seconds
///     0x80 | 0x18,               // frames
///     0x72,                      // cc_data id
///     0xe0 | 0x04,               // cc_count
///     0xFC, 0x20, 0x41,          // CEA608 field 1
///     0xFD, 0x42, 0x43,          // CEA608 field 2
///     0xFF, 0x02, 0x21,          // start CEA708 data
///     0xFE, 0x41, 0x00,
///     0x73,                      // svc_info id
///     0x80 | 0x40 | 0x10 | 0x01, // reserved | start | change | complete | count
///     0x80,                      // reserved | service number
///     b'e', b'n', b'g',          // language
///     0x40 | 0x3e,               // is_digital | reserved | field/service
///     0x3f,                      // reader | wide | reserved
///     0xff,                      // reserved
///     0x74,                      // cdp footer
///     0x12, 0x34,                // sequence counter
///     0xc4,                      // checksum
/// ];
/// parser.parse(&data).unwrap();
///
/// assert_eq!(parser.sequence(), 0x1234);
/// assert_eq!(parser.framerate(), Framerate::from_id(0x3));
///
/// // Service information
/// let service_info = parser.service_info().unwrap();
/// assert!(service_info.is_start());
/// assert!(!service_info.is_change());
/// assert!(service_info.is_complete());
/// let entries = service_info.services();
/// assert_eq!(entries[0].language(), [b'e', b'n', b'g']);
/// let FieldOrService::Field(field) = entries[0].service() else {
///     unreachable!();
/// };
/// assert!(field);
///
/// // Time code information
/// let time_code = parser.time_code().unwrap();
/// assert_eq!(time_code.hours(), 17);
/// assert_eq!(time_code.minutes(), 59);
/// assert_eq!(time_code.seconds(), 57);
/// assert_eq!(time_code.frames(), 18);
/// assert!(time_code.field());
/// assert!(time_code.drop_frame());
///
/// // CEA-708 cc_data
/// let packet = parser.pop_packet().unwrap();
/// assert_eq!(packet.sequence_no(), 0);
///
/// // CEA-608 data
/// let cea608 = parser.cea608().unwrap();
/// assert_eq!(cea608, &[Cea608::Field1(0x20, 0x41), Cea608::Field2(0x42, 0x43)]);
/// ```
#[derive(Debug)]
pub struct CDPParser {
    cc_data_parser: cea708_types::CCDataParser,
    time_code: Option<TimeCode>,
    framerate: Option<Framerate>,
    service_info: Option<ServiceInfo>,
    sequence: u16,
}

impl Default for CDPParser {
    fn default() -> Self {
        let mut cc_data_parser = cea708_types::CCDataParser::default();
        cc_data_parser.handle_cea608();
        Self {
            cc_data_parser,
            time_code: None,
            framerate: None,
            service_info: None,
            sequence: 0,
        }
    }
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
            let field = ((data[idx] & 0x80) >> 7) > 0;
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
            if data.len() < idx + svc_size {
                return Err(ParserError::LengthMismatch {
                    expected: idx + svc_size,
                    actual: data.len(),
                });
            }
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::tests::*;
    use crate::*;
    use cea708_types::{tables, Cea608};

    static PARSE_CDP: [TestCCData; 5] = [
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
                    field: true,
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
                    0x14,                      // cdp_len
                    0x3f,                      // framerate
                    0x20 | 0x10 | 0x04 | 0x01, // flags
                    0x12,                      // sequence counter
                    0x34,
                    0x73,                      // svc_info id
                    0x80 | 0x40 | 0x10 | 0x01, // reserved | start | change | complete | count
                    0x80,                      // reserved | service number
                    b'e',
                    b'n',
                    b'g',
                    0x40 | 0x3e, // is_digital | reserved | field/service
                    0x3f,        // reader | wide | reserved
                    0xff,        // reserved
                    0x74,        // cdp footer
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
        // simple packet with CEA-608 data
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
                    0xFC,
                    0x20,
                    0x41,
                    0xFD,
                    0x42,
                    0x80,
                    0x74, // cdp footer
                    0x12,
                    0x34,
                    0xFE, // checksum
                ],
                sequence_count: 0x1234,
                time_code: None,
                packets: &[],
                cea608: &[Cea608::Field1(0x20, 0x41), Cea608::Field2(0x42, 0x80)],
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
}
