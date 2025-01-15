// Copyright (C) 2025 Matthew Waters <matthew@centricular.com>
//
// Licensed under the MIT license <LICENSE-MIT> or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use crate::ParserError;
use crate::WriterError;

#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct ServiceInfo {
    start: bool,
    change: bool,
    complete: bool,
    services: Vec<ServiceEntry>,
}

impl ServiceInfo {
    /// Parse a sequence of bytes into a valid Service Descriptor.
    pub fn parse(data: &[u8]) -> Result<Self, ParserError> {
        if data.len() < 2 {
            return Err(ParserError::LengthMismatch {
                expected: 2,
                actual: data.len(),
            });
        }
        if data[0] != 0x73 {
            return Err(ParserError::WrongMagic);
        }
        if data[1] & 0x80 != 0x80 {
            return Err(ParserError::InvalidFixedBits);
        }
        let svc_count = (data[1] & 0xf) as usize;
        let expected = svc_count * 7 + 2;
        if data.len() != expected {
            return Err(ParserError::LengthMismatch {
                expected,
                actual: data.len(),
            });
        }
        let start = data[1] & 0x40 > 0;
        let change = data[1] & 0x20 > 0;
        let complete = data[1] & 0x10 > 0;
        let mut ret = Self {
            start,
            change,
            complete,
            services: vec![],
        };
        let mut data = &data[2..];
        for _ in 0..svc_count {
            trace!("parsing entry {:x?}", &data[..7]);
            if data[0] & 0x80 != 0x80 {
                return Err(ParserError::InvalidFixedBits);
            }
            let service_large = data[0] & 0x40 > 0;
            let service_no = if service_large {
                if data[0] & 0x20 != 0x20 {
                    return Err(ParserError::InvalidFixedBits);
                }
                data[0] & 0x1f
            } else {
                data[0] & 0x3f
            };
            let service =
                ServiceEntry::parse([data[1], data[2], data[3], data[4], data[5], data[6]])?;
            match &service.service {
                FieldOrService::Service(digital) => {
                    if digital.service != service_no {
                        return Err(ParserError::ServiceNumberMismatch);
                    }
                }
                FieldOrService::Field(_field1) => {
                    if service_no != 0 {
                        return Err(ParserError::ServiceNumberMismatch);
                    }
                }
            }
            data = &data[7..];
            ret.services.push(service);
        }
        Ok(ret)
    }

    /// This packet begins a complete set of Service Information.
    pub fn is_start(&self) -> bool {
        self.start
    }

    /// Set the start flag in this Service Information.
    pub fn set_start(&mut self, start: bool) {
        self.start = start;
    }

    /// This packet is an update to a previously sent Service Information.  Can only be `true`
    /// when [is_start](ServiceInfo::is_start) is also `true`.
    pub fn is_change(&self) -> bool {
        self.change
    }

    /// Set the change flag in this Service Information.  If true, then the start flag will also be
    /// set to true.
    pub fn set_change(&mut self, change: bool) {
        self.change = change;
        if change {
            self.start = true;
        }
    }

    /// This packet concludes a complete set of Service Information.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Set the complete flag in this Service Information.
    pub fn set_complete(&mut self, complete: bool) {
        self.complete = complete;
    }

    /// The list of services described by this Service Information.
    pub fn services(&self) -> &[ServiceEntry] {
        &self.services
    }

    /// Remove all services from this Service Information block.
    pub fn clear_services(&mut self) {
        self.services.clear();
    }

    /// Add a service to this Service Information block.
    pub fn add_service(&mut self, service: ServiceEntry) -> Result<(), WriterError> {
        if self.services.len() >= 15 {
            return Err(WriterError::WouldOverflow(1));
        }
        self.services.push(service);
        Ok(())
    }

    /// The length in bytes of this Service Information.
    pub fn byte_len(&self) -> usize {
        self.services.len() * 7 + 2
    }

    /// Write this Service Information to a sequence of bytes.
    pub fn write<W: std::io::Write>(&mut self, w: &mut W) -> Result<(), std::io::Error> {
        let mut header = [0; 2];
        self.write_header_unchecked(&mut header);
        w.write_all(&header)?;
        for svc in self.services.iter() {
            let mut data = [0; 7];
            self.write_svc_header_unchecked(svc, &mut data[..1]);
            svc.write_into_unchecked(&mut data[1..7]);
            w.write_all(&data)?;
        }
        Ok(())
    }

    fn write_header_unchecked(&self, data: &mut [u8]) {
        data[0] = 0x73;
        let mut byte = 0x80;
        if self.start {
            byte |= 0x40;
        }
        if self.change {
            byte |= 0x20;
        }
        if self.complete {
            byte |= 0x10;
        }
        let byte_len = self.services.len() & 0xf;
        byte |= byte_len as u8;
        data[1] = byte;
    }

    fn write_svc_header_unchecked(&self, svc: &ServiceEntry, data: &mut [u8]) {
        match &svc.service {
            FieldOrService::Field(field) => {
                let mut byte = 0x80;
                if !*field {
                    byte |= 0x01
                }
                data[0] = byte;
            }
            FieldOrService::Service(digital) => {
                data[0] = 0x80 | digital.service;
            }
        }
    }

    /// Write this Service Information into a preallocated sequence of bytes.  `data` must be at
    /// least [byte_len](ServiceInfo::byte_len) bytes.
    pub fn write_into_unchecked(&self, data: &mut [u8]) -> usize {
        self.write_header_unchecked(data);
        let mut idx = 2;
        for svc in self.services.iter() {
            self.write_svc_header_unchecked(svc, &mut data[idx..idx + 1]);
            svc.write_into_unchecked(&mut data[idx + 1..idx + 7]);
            idx += 7;
        }
        idx
    }
}

/// An entry for a caption service as specified in ATSC A/65 (2013) 6.9.2 Caption Service
/// Descriptor - Table 6.26
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ServiceEntry {
    language: [u8; 3],
    service: FieldOrService,
}

impl ServiceEntry {
    /// Construct a new [`ServiceEntry`].
    pub fn new(language: [u8; 3], service: FieldOrService) -> Self {
        Self { language, service }
    }

    /// Parse a Caption Service Descriptor as specified in ATSC A/65.
    pub fn parse(data: [u8; 6]) -> Result<Self, ParserError> {
        let digital_cc = data[3] & 0x80 > 0;
        if data[3] & 0x40 != 0x40 {
            return Err(ParserError::InvalidFixedBits);
        }
        let atsc_service_no = data[3] & 0x3f;
        let easy_reader = data[4] & 0x80 > 0;
        let wide_aspect_ratio = data[4] & 0x40 > 0;
        let service = if digital_cc {
            if atsc_service_no == 0 {
                return Err(ParserError::InvalidServiceNumber);
            }
            FieldOrService::Service(DigitalServiceEntry {
                service: atsc_service_no,
                easy_reader,
                wide_aspect_ratio,
            })
        } else {
            if data[3] & 0x3e != 0x3e {
                return Err(ParserError::InvalidFixedBits);
            }
            FieldOrService::Field(atsc_service_no & 0x01 == 0)
        };
        if data[4] & 0x3f != 0x3f {
            return Err(ParserError::InvalidFixedBits);
        }
        if data[5] != 0xff {
            return Err(ParserError::InvalidFixedBits);
        }
        Ok(Self {
            language: [data[0], data[1], data[2]],
            service,
        })
    }

    /// Language code as specified in ISO 639.2/B encoded in ISO 8859-1 (latin-1).
    pub fn language(&self) -> [u8; 3] {
        self.language
    }

    /// The CEA-608 field or CEA-708 service referenced by this entry.
    pub fn service(&self) -> &FieldOrService {
        &self.service
    }

    pub fn write<W: std::io::Write>(&mut self, w: &mut W) -> Result<(), std::io::Error> {
        let mut data = [0; 6];
        self.write_into_unchecked(&mut data);
        w.write_all(&data)
    }

    pub fn write_into_unchecked(&self, data: &mut [u8]) {
        data[0] = self.language[0];
        data[1] = self.language[1];
        data[2] = self.language[2];
        match &self.service {
            FieldOrService::Field(field) => {
                let mut byte = 0x7e;
                if !*field {
                    byte |= 0x01;
                }
                data[3] = byte;
                data[4] = 0x3f;
            }
            FieldOrService::Service(digital) => {
                data[3] = 0xc0 | digital.service;
                let mut byte = 0x3f;
                if digital.easy_reader {
                    byte |= 0x80;
                }
                if digital.wide_aspect_ratio {
                    byte |= 0x40;
                }
                data[4] = byte;
            }
        }
        data[5] = 0xff;
    }
}

/// A value that is either a CEA-608 field or a CEA-708 service.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum FieldOrService {
    /// A CEA-608 field. Field 1 == true, Field 2 == false.
    Field(bool),
    /// A CEA-708 service.
    Service(DigitalServiceEntry),
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct DigitalServiceEntry {
    service: u8,
    easy_reader: bool,
    wide_aspect_ratio: bool,
}

impl DigitalServiceEntry {
    pub fn new(service: u8, easy_reader: bool, wide_aspect_ratio: bool) -> Self {
        Self {
            service,
            easy_reader,
            wide_aspect_ratio,
        }
    }

    pub fn service_no(&self) -> u8 {
        self.service
    }

    pub fn easy_reader(&self) -> bool {
        self.easy_reader
    }

    pub fn wide_aspect_ratio(&self) -> bool {
        self.wide_aspect_ratio
    }
}

#[cfg(test)]
mod test {
    use std::sync::LazyLock;

    use super::*;
    use crate::tests::test_init_log;

    static LANG_TAG: [u8; 3] = [b'e', b'n', b'g'];

    #[derive(Debug)]
    struct TestSVCData {
        data: Vec<u8>,
        service_info: ServiceInfo,
    }

    static PARSE_SERVICE: LazyLock<[TestSVCData; 1]> = LazyLock::new(|| {
        [TestSVCData {
            data: vec![
                0x73, // magic
                0xd2, // start | change | complete | count
                0x80, // service_no
                LANG_TAG[0],
                LANG_TAG[1],
                LANG_TAG[2],
                0x7e, // is_digital | service_no
                0x3f, // easy reader | wide aspect_ratio
                0xff, // reserved
                0xe1,
                LANG_TAG[0],
                LANG_TAG[1],
                LANG_TAG[2],
                0xc1,
                0xff,
                0xff,
            ],
            service_info: ServiceInfo {
                start: true,
                change: false,
                complete: true,
                services: vec![
                    ServiceEntry {
                        language: LANG_TAG,
                        service: FieldOrService::Field(true),
                    },
                    ServiceEntry {
                        language: LANG_TAG,
                        service: FieldOrService::Service(DigitalServiceEntry {
                            service: 1,
                            easy_reader: true,
                            wide_aspect_ratio: true,
                        }),
                    },
                ],
            },
        }]
    });

    #[test]
    fn parse_service_descriptor() {
        test_init_log();

        for service in PARSE_SERVICE.iter() {
            debug!("parsing service info data: {:x?}", service.data);
            let parsed = ServiceInfo::parse(&service.data).unwrap();
            assert_eq!(parsed, service.service_info);
        }
    }

    #[test]
    fn roundtrip_service_descriptor() {
        test_init_log();

        for svc in PARSE_SERVICE.iter() {
            debug!("writing service {:?}", svc.service_info);
            debug!("existing data {:x?}", svc.data);
            let byte_len = svc.service_info.byte_len();
            let mut data = vec![0; byte_len];
            svc.service_info.write_into_unchecked(&mut data);
            debug!("wrote service data {data:x?}");
            let service = ServiceInfo::parse(&data).unwrap();
            debug!("parsed service {service:?}");
            assert_eq!(service, svc.service_info);
        }
    }

    #[test]
    fn add_service_overflow() {
        test_init_log();

        let mut info = ServiceInfo::default();
        let lang_tag = [b'e', b'n', b'g'];
        for i in 0..15 {
            let entry = ServiceEntry::new(
                lang_tag,
                FieldOrService::Service(DigitalServiceEntry::new(i, false, false)),
            );
            info.add_service(entry).unwrap();
        }
        let entry = ServiceEntry::new(
            lang_tag,
            FieldOrService::Service(DigitalServiceEntry::new(1, false, false)),
        );
        assert_eq!(info.add_service(entry), Err(WriterError::WouldOverflow(1)));
    }
}
