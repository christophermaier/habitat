// This file is generated. Do not edit
// @generated

// https://github.com/Manishearth/rust-clippy/issues/702
#![allow(unknown_lints)]
#![allow(clippy)]

#![cfg_attr(rustfmt, rustfmt_skip)]

#![allow(box_pointers)]
#![allow(dead_code)]
#![allow(missing_docs)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(trivial_casts)]
#![allow(unsafe_code)]
#![allow(unused_imports)]
#![allow(unused_results)]

use protobuf::Message as Message_imported_for_functions;
use protobuf::ProtobufEnum as ProtobufEnum_imported_for_functions;

#[derive(PartialEq,Clone,Default)]
pub struct Envelope {
    // message fields
    message_id: ::protobuf::SingularField<::std::string::String>,
    payload: ::protobuf::SingularField<::std::vec::Vec<u8>>,
    txn_id: ::std::option::Option<u64>,
    // special fields
    unknown_fields: ::protobuf::UnknownFields,
    cached_size: ::protobuf::CachedSize,
}

// see codegen.rs for the explanation why impl Sync explicitly
unsafe impl ::std::marker::Sync for Envelope {}

impl Envelope {
    pub fn new() -> Envelope {
        ::std::default::Default::default()
    }

    pub fn default_instance() -> &'static Envelope {
        static mut instance: ::protobuf::lazy::Lazy<Envelope> = ::protobuf::lazy::Lazy {
            lock: ::protobuf::lazy::ONCE_INIT,
            ptr: 0 as *const Envelope,
        };
        unsafe {
            instance.get(Envelope::new)
        }
    }

    // optional string message_id = 1;

    pub fn clear_message_id(&mut self) {
        self.message_id.clear();
    }

    pub fn has_message_id(&self) -> bool {
        self.message_id.is_some()
    }

    // Param is passed by value, moved
    pub fn set_message_id(&mut self, v: ::std::string::String) {
        self.message_id = ::protobuf::SingularField::some(v);
    }

    // Mutable pointer to the field.
    // If field is not initialized, it is initialized with default value first.
    pub fn mut_message_id(&mut self) -> &mut ::std::string::String {
        if self.message_id.is_none() {
            self.message_id.set_default();
        };
        self.message_id.as_mut().unwrap()
    }

    // Take field
    pub fn take_message_id(&mut self) -> ::std::string::String {
        self.message_id.take().unwrap_or_else(|| ::std::string::String::new())
    }

    pub fn get_message_id(&self) -> &str {
        match self.message_id.as_ref() {
            Some(v) => &v,
            None => "",
        }
    }

    fn get_message_id_for_reflect(&self) -> &::protobuf::SingularField<::std::string::String> {
        &self.message_id
    }

    fn mut_message_id_for_reflect(&mut self) -> &mut ::protobuf::SingularField<::std::string::String> {
        &mut self.message_id
    }

    // optional bytes payload = 2;

    pub fn clear_payload(&mut self) {
        self.payload.clear();
    }

    pub fn has_payload(&self) -> bool {
        self.payload.is_some()
    }

    // Param is passed by value, moved
    pub fn set_payload(&mut self, v: ::std::vec::Vec<u8>) {
        self.payload = ::protobuf::SingularField::some(v);
    }

    // Mutable pointer to the field.
    // If field is not initialized, it is initialized with default value first.
    pub fn mut_payload(&mut self) -> &mut ::std::vec::Vec<u8> {
        if self.payload.is_none() {
            self.payload.set_default();
        };
        self.payload.as_mut().unwrap()
    }

    // Take field
    pub fn take_payload(&mut self) -> ::std::vec::Vec<u8> {
        self.payload.take().unwrap_or_else(|| ::std::vec::Vec::new())
    }

    pub fn get_payload(&self) -> &[u8] {
        match self.payload.as_ref() {
            Some(v) => &v,
            None => &[],
        }
    }

    fn get_payload_for_reflect(&self) -> &::protobuf::SingularField<::std::vec::Vec<u8>> {
        &self.payload
    }

    fn mut_payload_for_reflect(&mut self) -> &mut ::protobuf::SingularField<::std::vec::Vec<u8>> {
        &mut self.payload
    }

    // optional uint64 txn_id = 3;

    pub fn clear_txn_id(&mut self) {
        self.txn_id = ::std::option::Option::None;
    }

    pub fn has_txn_id(&self) -> bool {
        self.txn_id.is_some()
    }

    // Param is passed by value, moved
    pub fn set_txn_id(&mut self, v: u64) {
        self.txn_id = ::std::option::Option::Some(v);
    }

    pub fn get_txn_id(&self) -> u64 {
        self.txn_id.unwrap_or(0)
    }

    fn get_txn_id_for_reflect(&self) -> &::std::option::Option<u64> {
        &self.txn_id
    }

    fn mut_txn_id_for_reflect(&mut self) -> &mut ::std::option::Option<u64> {
        &mut self.txn_id
    }
}

impl ::protobuf::Message for Envelope {
    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream) -> ::protobuf::ProtobufResult<()> {
        while !is.eof()? {
            let (field_number, wire_type) = is.read_tag_unpack()?;
            match field_number {
                1 => {
                    ::protobuf::rt::read_singular_string_into(wire_type, is, &mut self.message_id)?;
                },
                2 => {
                    ::protobuf::rt::read_singular_bytes_into(wire_type, is, &mut self.payload)?;
                },
                3 => {
                    if wire_type != ::protobuf::wire_format::WireTypeVarint {
                        return ::std::result::Result::Err(::protobuf::rt::unexpected_wire_type(wire_type));
                    };
                    let tmp = is.read_uint64()?;
                    self.txn_id = ::std::option::Option::Some(tmp);
                },
                _ => {
                    ::protobuf::rt::read_unknown_or_skip_group(field_number, wire_type, is, self.mut_unknown_fields())?;
                },
            };
        }
        ::std::result::Result::Ok(())
    }

    // Compute sizes of nested messages
    #[allow(unused_variables)]
    fn compute_size(&self) -> u32 {
        let mut my_size = 0;
        if let Some(v) = self.message_id.as_ref() {
            my_size += ::protobuf::rt::string_size(1, &v);
        };
        if let Some(v) = self.payload.as_ref() {
            my_size += ::protobuf::rt::bytes_size(2, &v);
        };
        if let Some(v) = self.txn_id {
            my_size += ::protobuf::rt::value_size(3, v, ::protobuf::wire_format::WireTypeVarint);
        };
        my_size += ::protobuf::rt::unknown_fields_size(self.get_unknown_fields());
        self.cached_size.set(my_size);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream) -> ::protobuf::ProtobufResult<()> {
        if let Some(v) = self.message_id.as_ref() {
            os.write_string(1, &v)?;
        };
        if let Some(v) = self.payload.as_ref() {
            os.write_bytes(2, &v)?;
        };
        if let Some(v) = self.txn_id {
            os.write_uint64(3, v)?;
        };
        os.write_unknown_fields(self.get_unknown_fields())?;
        ::std::result::Result::Ok(())
    }

    fn get_cached_size(&self) -> u32 {
        self.cached_size.get()
    }

    fn get_unknown_fields(&self) -> &::protobuf::UnknownFields {
        &self.unknown_fields
    }

    fn mut_unknown_fields(&mut self) -> &mut ::protobuf::UnknownFields {
        &mut self.unknown_fields
    }

    fn as_any(&self) -> &::std::any::Any {
        self as &::std::any::Any
    }
    fn as_any_mut(&mut self) -> &mut ::std::any::Any {
        self as &mut ::std::any::Any
    }
    fn into_any(self: Box<Self>) -> ::std::boxed::Box<::std::any::Any> {
        self
    }

    fn descriptor(&self) -> &'static ::protobuf::reflect::MessageDescriptor {
        ::protobuf::MessageStatic::descriptor_static(None::<Self>)
    }
}

impl ::protobuf::MessageStatic for Envelope {
    fn new() -> Envelope {
        Envelope::new()
    }

    fn descriptor_static(_: ::std::option::Option<Envelope>) -> &'static ::protobuf::reflect::MessageDescriptor {
        static mut descriptor: ::protobuf::lazy::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::lazy::Lazy {
            lock: ::protobuf::lazy::ONCE_INIT,
            ptr: 0 as *const ::protobuf::reflect::MessageDescriptor,
        };
        unsafe {
            descriptor.get(|| {
                let mut fields = ::std::vec::Vec::new();
                fields.push(::protobuf::reflect::accessor::make_singular_field_accessor::<_, ::protobuf::types::ProtobufTypeString>(
                    "message_id",
                    Envelope::get_message_id_for_reflect,
                    Envelope::mut_message_id_for_reflect,
                ));
                fields.push(::protobuf::reflect::accessor::make_singular_field_accessor::<_, ::protobuf::types::ProtobufTypeBytes>(
                    "payload",
                    Envelope::get_payload_for_reflect,
                    Envelope::mut_payload_for_reflect,
                ));
                fields.push(::protobuf::reflect::accessor::make_option_accessor::<_, ::protobuf::types::ProtobufTypeUint64>(
                    "txn_id",
                    Envelope::get_txn_id_for_reflect,
                    Envelope::mut_txn_id_for_reflect,
                ));
                ::protobuf::reflect::MessageDescriptor::new::<Envelope>(
                    "Envelope",
                    fields,
                    file_descriptor_proto()
                )
            })
        }
    }
}

impl ::protobuf::Clear for Envelope {
    fn clear(&mut self) {
        self.clear_message_id();
        self.clear_payload();
        self.clear_txn_id();
        self.unknown_fields.clear();
    }
}

impl ::std::fmt::Debug for Envelope {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for Envelope {
    fn as_ref(&self) -> ::protobuf::reflect::ProtobufValueRef {
        ::protobuf::reflect::ProtobufValueRef::Message(self)
    }
}

static file_descriptor_proto_data: &'static [u8] = &[
    0x0a, 0x13, 0x70, 0x72, 0x6f, 0x74, 0x6f, 0x63, 0x6f, 0x6c, 0x73, 0x2f, 0x6e, 0x65, 0x74, 0x2e,
    0x70, 0x72, 0x6f, 0x74, 0x6f, 0x12, 0x03, 0x6e, 0x65, 0x74, 0x22, 0x5a, 0x0a, 0x08, 0x45, 0x6e,
    0x76, 0x65, 0x6c, 0x6f, 0x70, 0x65, 0x12, 0x1d, 0x0a, 0x0a, 0x6d, 0x65, 0x73, 0x73, 0x61, 0x67,
    0x65, 0x5f, 0x69, 0x64, 0x18, 0x01, 0x20, 0x01, 0x28, 0x09, 0x52, 0x09, 0x6d, 0x65, 0x73, 0x73,
    0x61, 0x67, 0x65, 0x49, 0x64, 0x12, 0x18, 0x0a, 0x07, 0x70, 0x61, 0x79, 0x6c, 0x6f, 0x61, 0x64,
    0x18, 0x02, 0x20, 0x01, 0x28, 0x0c, 0x52, 0x07, 0x70, 0x61, 0x79, 0x6c, 0x6f, 0x61, 0x64, 0x12,
    0x15, 0x0a, 0x06, 0x74, 0x78, 0x6e, 0x5f, 0x69, 0x64, 0x18, 0x03, 0x20, 0x01, 0x28, 0x04, 0x52,
    0x05, 0x74, 0x78, 0x6e, 0x49, 0x64, 0x4a, 0x83, 0x02, 0x0a, 0x06, 0x12, 0x04, 0x00, 0x00, 0x08,
    0x01, 0x0a, 0x08, 0x0a, 0x01, 0x0c, 0x12, 0x03, 0x00, 0x00, 0x12, 0x0a, 0x08, 0x0a, 0x01, 0x02,
    0x12, 0x03, 0x02, 0x08, 0x0b, 0x0a, 0x0a, 0x0a, 0x02, 0x04, 0x00, 0x12, 0x04, 0x04, 0x00, 0x08,
    0x01, 0x0a, 0x0a, 0x0a, 0x03, 0x04, 0x00, 0x01, 0x12, 0x03, 0x04, 0x08, 0x10, 0x0a, 0x0b, 0x0a,
    0x04, 0x04, 0x00, 0x02, 0x00, 0x12, 0x03, 0x05, 0x02, 0x21, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00,
    0x02, 0x00, 0x04, 0x12, 0x03, 0x05, 0x02, 0x0a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x00,
    0x05, 0x12, 0x03, 0x05, 0x0b, 0x11, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x00, 0x01, 0x12,
    0x03, 0x05, 0x12, 0x1c, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x00, 0x03, 0x12, 0x03, 0x05,
    0x1f, 0x20, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x01, 0x12, 0x03, 0x06, 0x02, 0x1d, 0x0a,
    0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x01, 0x04, 0x12, 0x03, 0x06, 0x02, 0x0a, 0x0a, 0x0c, 0x0a,
    0x05, 0x04, 0x00, 0x02, 0x01, 0x05, 0x12, 0x03, 0x06, 0x0b, 0x10, 0x0a, 0x0c, 0x0a, 0x05, 0x04,
    0x00, 0x02, 0x01, 0x01, 0x12, 0x03, 0x06, 0x11, 0x18, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02,
    0x01, 0x03, 0x12, 0x03, 0x06, 0x1b, 0x1c, 0x0a, 0x0b, 0x0a, 0x04, 0x04, 0x00, 0x02, 0x02, 0x12,
    0x03, 0x07, 0x02, 0x1d, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x02, 0x04, 0x12, 0x03, 0x07,
    0x02, 0x0a, 0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x02, 0x05, 0x12, 0x03, 0x07, 0x0b, 0x11,
    0x0a, 0x0c, 0x0a, 0x05, 0x04, 0x00, 0x02, 0x02, 0x01, 0x12, 0x03, 0x07, 0x12, 0x18, 0x0a, 0x0c,
    0x0a, 0x05, 0x04, 0x00, 0x02, 0x02, 0x03, 0x12, 0x03, 0x07, 0x1b, 0x1c,
];

static mut file_descriptor_proto_lazy: ::protobuf::lazy::Lazy<::protobuf::descriptor::FileDescriptorProto> = ::protobuf::lazy::Lazy {
    lock: ::protobuf::lazy::ONCE_INIT,
    ptr: 0 as *const ::protobuf::descriptor::FileDescriptorProto,
};

fn parse_descriptor_proto() -> ::protobuf::descriptor::FileDescriptorProto {
    ::protobuf::parse_from_bytes(file_descriptor_proto_data).unwrap()
}

pub fn file_descriptor_proto() -> &'static ::protobuf::descriptor::FileDescriptorProto {
    unsafe {
        file_descriptor_proto_lazy.get(|| {
            parse_descriptor_proto()
        })
    }
}
