// Copyright (C) 2025 Matthew Waters <matthew@centricular.com>
//
// Licensed under the MIT license <LICENSE-MIT> or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use crate::{Flags, Framerate, ServiceInfo, TimeCode};

/// A struct for writing a stream of CDPs
///
/// # Examples
///
/// ```
/// # use cdp_types::*;
/// use cdp_types::cea708_types::{Cea608, DTVCCPacket, Service, tables};
///
/// let mut writer = CDPWriter::new();
/// writer.set_sequence_count(3);
/// let mut packet = DTVCCPacket::new(0);
/// let mut service = Service::new(1);
/// service.push_code(&tables::Code::LatinCapitalA).unwrap();
/// packet.push_service(service).unwrap();
/// writer.push_packet(packet);
///
/// writer.push_cea608(Cea608::Field1(0x41, 0x80));
///
/// writer.set_time_code(Some(TimeCode::new(1, 2, 3, 4, true, false)));
///
/// let mut service_info = ServiceInfo::default();
/// service_info.set_start(true);
/// service_info.set_complete(true);
/// let entry = ServiceEntry::new([b'e', b'n', b'g'], FieldOrService::Field(true));
/// service_info.add_service(entry);
/// let entry = ServiceEntry::new(
///     [b'e', b'n', b'g'],
///     FieldOrService::Service(DigitalServiceEntry::new(1, false, true))
/// );
/// service_info.add_service(entry);
/// writer.set_service_info(Some(service_info));
///
/// let framerate = Framerate::from_id(4).unwrap();
/// let mut data = vec![];
/// writer.write(framerate, &mut data).unwrap();
///
/// let expected = [
///     0x96, 0x69,         // magic
///     0x5e,               // CDP length
///     0x4f,               // framerate
///     0xf7,               // flags
///     0x00, 0x03,         // sequence counter
///     0x71,               // time code start
///     0xc1,               // hours
///     0x82,               // minutes
///     0x83,               // seconds
///     0x04,               // frames
///     0x72,               // cc_data id
///     0xf4,               // cc_data count
///     0xfc, 0x41, 0x80,   // CEA-608 field 1
///     0xf9, 0x80, 0x80,   // CEA-608 field 2
///     0xff, 0x02, 0x21,   // CEA-708 start
///     0xfe, 0x41, 0x00,   // CEA-708 continued
///     0xfa, 0x00, 0x00,   // CEA-708 padding
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0xfa, 0x00, 0x00,   // .
///     0x73,               // service info id
///     0xd2,               // start | change | complete | count
///     0x80,               // service no
///     b'e', b'n', b'g',   // language
///     0x7e,               // is_digital | ignored
///     0x3f, 0xff,         // ignored | reserved
///     0x81,               // service no
///     b'e', b'n', b'g',   // language
///     0xc1,               // is_digital | service no
///     0x7f, 0xff,         // easy_reader | wide_aspect_ratio | reserved
///     0x74,               // footer id
///     0x00, 0x03,         // sequence counter
///     0xd6,               // checksum
/// ];
/// assert_eq!(&data, &expected);
/// ```
#[derive(Debug)]
pub struct CDPWriter {
    cc_data: cea708_types::CCDataWriter,
    time_code: Option<TimeCode>,
    service_info: Option<ServiceInfo>,
    sequence_count: u16,
}

impl Default for CDPWriter {
    fn default() -> Self {
        let mut cc_data = cea708_types::CCDataWriter::default();
        cc_data.set_output_padding(true);
        cc_data.set_output_cea608_padding(true);
        Self {
            cc_data,
            time_code: None,
            service_info: None,
            sequence_count: 0,
        }
    }
}

impl CDPWriter {
    /// Construct a new [`CDPWriter`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a [`cea708_types::DTVCCPacket`] for writing
    pub fn push_packet(&mut self, packet: cea708_types::DTVCCPacket) {
        self.cc_data.push_packet(packet)
    }

    /// Push a [`cea708_types::Cea608`] byte pair for writing
    pub fn push_cea608(&mut self, cea608: cea708_types::Cea608) {
        self.cc_data.push_cea608(cea608)
    }

    /// Set the optional time code to use for the next CDP packet that is generated.
    pub fn set_time_code(&mut self, time_code: Option<TimeCode>) {
        self.time_code = time_code;
    }

    /// Set the optional [`ServiceInfo`] for the next CDP packet that is generated.
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
        self.service_info = None;
    }

    /// Write the next CDP packet taking the next relevant CEA-608 byte pairs and
    /// [`cea708_types::DTVCCPacket`]s.
    pub fn write<W: std::io::Write>(
        &mut self,
        framerate: Framerate,
        w: &mut W,
    ) -> Result<(), std::io::Error> {
        let mut len = 7; // header
        if self.time_code.is_some() {
            len += 5;
        }
        let mut cc_data = Vec::new();
        self.cc_data.write(
            cea708_types::Framerate::new(framerate.numer(), framerate.denom()),
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

        let mut flags = Flags::CC_DATA_PRESENT | Flags::CAPTION_SERVICE_ACTIVE | 0x1;
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
            framerate.id << 4 | 0x0f,
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
                if time_code.field { 0x80 } else { 0x00 }
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::tests::*;
    use crate::*;
    use cea708_types::{tables, DTVCCPacket, Service};

    static WRITE_CDP: [TestCCData; 2] = [
        // simple packet with a single service and single code
        TestCCData {
            framerate: FRAMERATES[2],
            cdp_data: &[CDPPacketData {
                data: &[
                    0x96,
                    0x69,                      // magic
                    0x5A,                      // cdp_len
                    0x3f,                      //framerate
                    0x80 | 0x40 | 0x02 | 0x01, // flags
                    0x12,
                    0x34,        // sequence counter
                    0x71,        // time code id
                    0xc0 | 0x17, // hours
                    0x80 | 0x59, // minutes
                    0x80 | 0x57, // seconds
                    0x80 | 0x18, // frames
                    0x72,        // cc_data id
                    0xe0 | 0x18,
                    0xF8,
                    0x80,
                    0x80,
                    0xF9,
                    0x80,
                    0x80,
                    0xFF,
                    0x02,
                    0x21,
                    0xFE,
                    0x41,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0x74, // footer
                    0x12,
                    0x34,
                    0xD1, //checksum
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
        TestCCData {
            framerate: FRAMERATES[2],
            cdp_data: &[CDPPacketData {
                data: &[
                    0x96, // magic
                    0x69,
                    0x55,               // cdp_len
                    0x3f,               // framerate
                    0x40 | 0x02 | 0x01, // flags
                    0x34,               // sequence counter
                    0x12,
                    0x72,        // cc_data id
                    0xe0 | 0x18, // cc_count
                    0xF8,
                    0x80,
                    0x80,
                    0xF9,
                    0x80,
                    0x80,
                    0xFF,
                    0x02,
                    0x21,
                    0xFE,
                    0x41,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0xFA,
                    0x00,
                    0x00,
                    0x74, // cdp footer
                    0x34,
                    0x12,
                    0xE6, // checksum
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
            let mut writer = CDPWriter::new();
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
                writer.write(test_data.framerate, &mut written).unwrap();
                assert_eq!(cdp_data.data, &written);
            }
        }
    }
}
