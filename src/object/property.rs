use crate::object::data_type::DataType;
use itertools::Itertools;
use std::convert::TryInto;
use std::{mem, slice};

/*
Binary format:

All numbers are little endian!

-- STATIC DATA --
bool1-N: u8

padding: offset % 4

int1-N: i32
float1-N: f32

padding: offset % 8

long1-N: i64
double1-N: f64

-- POINTERS --
int_list_offset: u32 (relative to beginning) OR 0 for null list
int_list_length: u32 OR 0 for null list

long_list_offset
long_list_length

float_list_offset
float_list_length

double_list_offset
double_list_length

bool_list_offset
bool_list_length

string_offset: u32 (relative to beginning) OR 0 for null string
string_length: u32 number of BYTES OR 0 for null string

bytes_offset: u32 (relative to beginning) OR 0 for null bytes
bytes_length: u32 number of bytes OR 0 for null bytes

padding: (len(bool_lists) + len(string lists) + len(bytes_lists)) % 4
 */

#[repr(C)]
struct DataPosition {
    pub offset: u32,
    pub length: u32,
}

impl DataPosition {
    pub fn is_null(&self) -> bool {
        self.offset == 0
    }
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub struct Property {
    pub data_type: DataType,
    pub offset: usize,
}

impl Property {
    pub const NULL_INT: i32 = i32::MIN;
    pub const NULL_LONG: i64 = i64::MIN;
    pub const NULL_FLOAT: f32 = f32::NAN;
    pub const NULL_DOUBLE: f64 = f64::NAN;
    pub const FALSE_BOOL: u8 = 0;
    pub const TRUE_BOOL: u8 = 1;
    pub const NULL_BOOL: u8 = 2;

    pub fn new(data_type: DataType, offset: usize) -> Self {
        Property { data_type, offset }
    }

    #[inline]
    pub fn is_null(&self, object: &[u8]) -> bool {
        match self.data_type {
            DataType::Int => self.get_int(object) == Self::NULL_INT,
            DataType::Long => self.get_long(object) == Self::NULL_LONG,
            DataType::Float => self.get_float(object).is_nan(),
            DataType::Double => self.get_double(object).is_nan(),
            DataType::Bool => self.get_bool(object).is_none(),
            _ => self.get_length(object).is_none(),
        }
    }

    #[inline]
    pub fn get_int(&self, object: &[u8]) -> i32 {
        assert_eq!(self.data_type, DataType::Int);
        let bytes: [u8; 4] = object[self.offset..self.offset + 4].try_into().unwrap();
        i32::from_le_bytes(bytes)
    }

    #[inline]
    pub fn get_long(&self, object: &[u8]) -> i64 {
        assert_eq!(self.data_type, DataType::Long);
        let bytes: [u8; 8] = object[self.offset..self.offset + 8].try_into().unwrap();
        i64::from_le_bytes(bytes)
    }

    #[inline]
    pub fn get_float(&self, object: &[u8]) -> f32 {
        assert_eq!(self.data_type, DataType::Float);
        let bytes: [u8; 4] = object[self.offset..self.offset + 4].try_into().unwrap();
        f32::from_le_bytes(bytes)
    }

    #[inline]
    pub fn get_double(&self, object: &[u8]) -> f64 {
        assert_eq!(self.data_type, DataType::Double);
        let bytes: [u8; 8] = object[self.offset..self.offset + 8].try_into().unwrap();
        f64::from_le_bytes(bytes)
    }

    #[inline]
    pub fn get_bool(&self, object: &[u8]) -> Option<bool> {
        assert_eq!(self.data_type, DataType::Bool);
        match object[self.offset] {
            Self::FALSE_BOOL => Some(false),
            Self::TRUE_BOOL => Some(true),
            _ => None,
        }
    }

    #[inline]
    pub fn get_length(&self, object: &[u8]) -> Option<usize> {
        assert!(self.data_type.is_dynamic());
        let data_position = self.get_list_position(object, self.offset);
        if !data_position.is_null() {
            Some(data_position.length as usize)
        } else {
            None
        }
    }

    pub fn get_string<'a>(&self, object: &'a [u8]) -> Option<&'a str> {
        assert_eq!(self.data_type, DataType::String);
        let bytes = self.get_list::<u8>(object, self.offset)?;
        Some(std::str::from_utf8(bytes).unwrap())
    }

    pub fn get_bytes<'a>(&self, object: &'a [u8]) -> Option<&'a [u8]> {
        assert_eq!(self.data_type, DataType::Bytes);
        self.get_list(object, self.offset)
    }

    pub fn get_int_list<'a>(&self, object: &'a [u8]) -> Option<&'a [i32]> {
        assert_eq!(self.data_type, DataType::IntList);
        self.get_list(object, self.offset)
    }

    pub fn get_long_list<'a>(&self, object: &'a [u8]) -> Option<&'a [i64]> {
        assert_eq!(self.data_type, DataType::LongList);
        self.get_list(object, self.offset)
    }

    pub fn get_float_list<'a>(&self, object: &'a [u8]) -> Option<&'a [f32]> {
        assert_eq!(self.data_type, DataType::FloatList);
        self.get_list(object, self.offset)
    }

    pub fn get_double_list<'a>(&self, object: &'a [u8]) -> Option<&'a [f64]> {
        assert_eq!(self.data_type, DataType::DoubleList);
        self.get_list(object, self.offset)
    }

    /*pub fn get_bytes_list_positions<'a>(&self, object: &'a [u8]) -> Option<&'a [DataPosition]> {
        self.get_list(object, self.offset)
    }*/

    pub fn get_bytes_list<'a>(&self, object: &'a [u8]) -> Option<Vec<Option<&'a [u8]>>> {
        let positions_offset = self.get_list_position(object, self.offset);
        if positions_offset.is_null() {
            return None;
        }
        let lists = (0..positions_offset.length)
            .map(|i| {
                let list_offset = positions_offset.offset + i;
                self.get_list(object, list_offset as usize)
            })
            .collect_vec();
        Some(lists)
    }

    #[inline]
    fn get_list_position<'a>(&self, object: &'a [u8], offset: usize) -> &'a DataPosition {
        let bytes = &object[offset..offset + 8];
        &Self::transmute_verify_alignment::<DataPosition>(bytes)[0]
    }

    fn get_list<'a, T>(&self, object: &'a [u8], offset: usize) -> Option<&'a [T]> {
        let data_position = self.get_list_position(object, offset);
        if data_position.is_null() {
            return None;
        }
        let type_size = mem::size_of::<T>();
        let offset = data_position.offset as usize;
        let len_in_bytes = data_position.length as usize * type_size;
        let list_bytes = &object[offset..offset + len_in_bytes];
        Some(&Self::transmute_verify_alignment::<T>(list_bytes))
    }

    fn transmute_verify_alignment<T>(bytes: &[u8]) -> &[T] {
        let type_size = mem::size_of::<T>();
        let alignment = bytes.as_ref().as_ptr() as usize;
        assert_eq!(alignment % type_size, 0, "Wrong alignment.");
        let ptr = bytes.as_ptr() as *const u8;
        unsafe { slice::from_raw_parts::<T>(ptr as *const T, bytes.len() / type_size) }
    }

    pub(crate) fn get_static_raw<'a>(&self, object: &'a [u8]) -> &'a [u8] {
        match self.data_type {
            DataType::Int | DataType::Float => &object[self.offset..self.offset + 4],
            DataType::Bool => &object[self.offset..self.offset],
            _ => &object[self.offset..self.offset + 8],
        }
    }

    pub(crate) fn get_dynamic_raw<'a>(&self, object: &'a [u8]) -> Option<&'a [u8]> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use crate::object::property::{DataType, Property};
    use std::mem;

    #[repr(C, align(8))]
    struct Align8([u8; 8]);

    fn align(bytes: &[u8]) -> Vec<u8> {
        let n_units = (bytes.len() / mem::size_of::<Align8>()) + 1;

        let mut aligned: Vec<Align8> = Vec::with_capacity(n_units);

        let ptr = aligned.as_mut_ptr();
        let len_units = aligned.len();
        let cap_units = aligned.capacity();

        mem::forget(aligned);

        let mut vec = unsafe {
            Vec::from_raw_parts(
                ptr as *mut u8,
                len_units * mem::size_of::<Align8>(),
                cap_units * mem::size_of::<Align8>(),
            )
        };
        vec.extend_from_slice(bytes);
        vec
    }

    #[test]
    fn test_int_is_null() {
        let property = Property::new(DataType::Int, 0);

        let null_bytes = i32::to_le_bytes(Property::NULL_INT);
        assert!(property.is_null(&null_bytes));

        let bytes = i32::to_le_bytes(0);
        assert!(!property.is_null(&bytes));
    }

    #[test]
    fn test_long_is_null() {
        let property = Property::new(DataType::Long, 0);

        let null_bytes = i64::to_le_bytes(Property::NULL_LONG);
        assert!(property.is_null(&null_bytes));

        let bytes = i64::to_le_bytes(0);
        assert!(!property.is_null(&bytes));
    }

    #[test]
    fn test_float_is_null() {
        let property = Property::new(DataType::Float, 0);

        let null_bytes = f32::to_le_bytes(Property::NULL_FLOAT);
        assert!(property.is_null(&null_bytes));

        let bytes = i32::to_le_bytes(0);
        assert!(!property.is_null(&bytes));
    }

    #[test]
    fn test_double_is_null() {
        let property = Property::new(DataType::Double, 0);

        let null_bytes = f64::to_le_bytes(f64::NAN);
        assert!(property.is_null(&null_bytes));

        let bytes = f64::to_le_bytes(0.0);
        assert!(!property.is_null(&bytes));
    }

    #[test]
    fn test_bool_is_null() {
        let property = Property::new(DataType::Bool, 0);

        let null_bytes = [123];
        assert!(property.is_null(&null_bytes));

        let bytes = [0];
        assert!(!property.is_null(&bytes));

        let bytes = [1];
        assert!(!property.is_null(&bytes));
    }

    #[test]
    fn test_list_is_null() {
        let property = Property::new(DataType::String, 0);

        let null_bytes = align(&[0, 0, 0, 0, 1, 0, 0, 0]);
        assert!(property.is_null(&null_bytes));

        let bytes = align(&[8, 0, 0, 0, 1, 0, 0, 0, 42]);
        assert!(!property.is_null(&bytes));
    }

    #[test]
    fn test_get_int() {
        let property = Property::new(DataType::Int, 0);

        let bytes = i32::to_le_bytes(123);
        assert_eq!(property.get_int(&bytes), 123);

        let null_bytes = i32::to_le_bytes(Property::NULL_INT);
        assert_eq!(property.get_int(&null_bytes), Property::NULL_INT);
    }

    #[test]
    fn test_get_long() {
        let property = Property::new(DataType::Long, 0);

        let bytes = i64::to_le_bytes(123123123123123123);
        assert_eq!(property.get_long(&bytes), 123123123123123123);

        let null_bytes = i64::to_le_bytes(Property::NULL_LONG);
        assert_eq!(property.get_long(&null_bytes), Property::NULL_LONG);
    }

    #[test]
    fn test_get_float() {
        let property = Property::new(DataType::Float, 0);

        let bytes = f32::to_le_bytes(123.123);
        assert!((property.get_float(&bytes) - 123.123).abs() < std::f32::consts::TAU);

        let null_bytes = f32::to_le_bytes(Property::NULL_FLOAT);
        assert!(property.get_float(&null_bytes).is_nan());
    }

    #[test]
    fn test_get_double() {
        let property = Property::new(DataType::Double, 0);

        let bytes = f64::to_le_bytes(123123.123123123);
        assert!((property.get_double(&bytes) - 123123.123123123).abs() < std::f64::consts::TAU);

        let null_bytes = f64::to_le_bytes(Property::NULL_DOUBLE);
        assert!(property.get_double(&null_bytes).is_nan());
    }

    #[test]
    fn test_get_bool() {
        let property = Property::new(DataType::Bool, 0);

        let bytes = [0];
        assert_eq!(property.get_bool(&bytes), Some(false));

        let bytes = [1];
        assert_eq!(property.get_bool(&bytes), Some(true));

        let null_bytes = [2];
        assert_eq!(property.get_bool(&null_bytes), None);
    }

    #[test]
    fn test_get_length() {
        let property = Property::new(DataType::IntList, 0);

        let bytes = align(&[8, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(property.get_length(&bytes), Some(1));

        let bytes = align(&[0, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(property.get_length(&bytes), None);
    }

    #[test]
    fn test_get_string() {
        let property = Property::new(DataType::String, 0);

        let mut bytes = vec![8, 0, 0, 0, 5, 0, 0, 0];
        bytes.extend_from_slice("hello".as_bytes());
        assert_eq!(property.get_string(&bytes), Some("hello"));

        let bytes = [0, 0, 0, 0, 2, 0, 0, 0];
        assert_eq!(property.get_string(&bytes), None);
    }

    #[test]
    fn test_get_bytes() {
        let property = Property::new(DataType::Bytes, 0);

        let mut bytes = vec![8, 0, 0, 0, 5, 0, 0, 0];
        bytes.extend_from_slice("hello".as_bytes());
        assert_eq!(property.get_bytes(&bytes), Some("hello".as_bytes()));

        let bytes = [0, 0, 0, 0, 2, 0, 0, 0];
        assert_eq!(property.get_bytes(&bytes), None);
    }

    #[test]
    fn test_get_int_list() {
        let property = Property::new(DataType::IntList, 0);

        let bytes = align(&[8, 0, 0, 0, 2, 0, 0, 0, 5, 0, 0, 0, 6, 0, 0, 0]);
        assert_eq!(property.get_int_list(&bytes), Some(&[5i32, 6][..]));

        let bytes = align(&[0, 0, 0, 0, 2, 0, 0, 0]);
        assert_eq!(property.get_int_list(&bytes), None);
    }

    #[test]
    fn test_get_long_list() {
        let property = Property::new(DataType::LongList, 0);

        let bytes = align(&[
            8, 0, 0, 0, 2, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
        ]);
        assert_eq!(property.get_long_list(&bytes), Some(&[5i64, 6][..]));

        let bytes = align(&[0, 0, 0, 0, 2, 0, 0, 0]);
        assert_eq!(property.get_long_list(&bytes), None);
    }

    #[test]
    fn test_get_float_list() {
        let property = Property::new(DataType::FloatList, 0);

        let mut bytes = vec![8, 0, 0, 0, 2, 0, 0, 0];
        bytes.extend_from_slice(&10.5f32.to_le_bytes());
        bytes.extend_from_slice(&20.6f32.to_le_bytes());
        let bytes = align(&bytes);
        assert_eq!(property.get_float_list(&bytes), Some(&[10.5f32, 20.6][..]));

        let bytes = align(&[0, 0, 0, 0, 2, 0, 0, 0]);
        assert_eq!(property.get_float_list(&bytes), None);
    }

    #[test]
    fn test_get_double_list() {
        let property = Property::new(DataType::DoubleList, 0);

        let mut bytes = vec![8, 0, 0, 0, 2, 0, 0, 0];
        bytes.extend_from_slice(&10.5f64.to_le_bytes());
        bytes.extend_from_slice(&20.6f64.to_le_bytes());
        let bytes = align(&bytes);
        assert_eq!(property.get_double_list(&bytes), Some(&[10.5f64, 20.6][..]));

        let bytes = align(&[0, 0, 0, 0, 2, 0, 0, 0]);
        assert_eq!(property.get_double_list(&bytes), None);
    }

    /*#[test]
    fn test_string_property_is_null() {
        let property = Property::new(DataType::String, 0);
        let null_bytes = u32::to_le_bytes(NULL_LENGTH);
        assert!(property.is_null(&null_bytes));

        let bytes = [0, 0, 0, 0];
        assert_eq!(property.is_null(&bytes), false);
    }

    #[test]
    fn test_bytes_property_is_null() {
        let property = Property::new(DataType::Bytes, 0);
        let null_bytes = u32::to_le_bytes(NULL_LENGTH);
        assert!(property.is_null(&null_bytes));

        let bytes = [0, 0, 0, 0];
        assert_eq!(property.is_null(&bytes), false);
    }*/
}
