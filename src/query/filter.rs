use crate::collection::IsarCollection;
use crate::error::{illegal_arg, IsarError, Result};
use crate::link::Link;
use crate::lmdb::cursor::Cursor;
use crate::object::isar_object::{IsarObject, Property};
use crate::query::fast_wild_match::fast_wild_match;
use enum_dispatch::enum_dispatch;
use hashbrown::HashSet;
use paste::paste;

#[enum_dispatch]
#[derive(Clone)]
pub enum Filter {
    ByteBetween(ByteBetweenCond),
    IntBetween(IntBetweenCond),
    LongBetween(LongBetweenCond),
    FloatBetween(FloatBetweenCond),
    DoubleBetween(DoubleBetweenCond),

    ByteListContains(ByteListContainsCond),
    IntListContains(IntListContainsCond),
    LongListContains(LongListContainsCond),

    StringBetween(StringBetweenCond),
    StringStartsWith(StringStartsWithCond),
    StringEndsWith(StringEndsWithCond),
    StringMatches(StringMatchesCond),

    StringListContains(StringListContainsCond),

    And(AndCond),
    Or(OrCond),
    Not(NotCond),
    Static(StaticCond),
    Link(LinkCond),
}

pub(crate) struct FilterCursors<'txn, 'a>(&'a mut Cursor<'txn>, &'a mut Cursor<'txn>);

impl<'txn, 'a> FilterCursors<'txn, 'a> {
    pub fn new(primary: &'a mut Cursor<'txn>, links: &'a mut Cursor<'txn>) -> Self {
        FilterCursors(primary, links)
    }
}

#[enum_dispatch(Filter)]
pub(crate) trait Condition {
    fn evaluate(&self, object: IsarObject, cursors: Option<&mut FilterCursors>) -> Result<bool>;

    fn get_linked_collections(&self, col_ids: &mut HashSet<u16>);
}

#[macro_export]
macro_rules! filter_between_struct {
    ($name:ident, $data_type:ident, $type:ty) => {
        paste! {
            #[derive(Clone)]
            pub struct [<$name Cond>] {
                upper: $type,
                lower: $type,
                property: Property,
            }

            impl [<$name Cond>] {
                pub fn filter(property: Property, lower: $type, upper: $type) -> Result<Filter> {
                    if property.data_type == crate::object::data_type::DataType::$data_type {
                        Ok(Filter::$name(Self {
                            property,
                            lower,
                            upper,
                        }))
                    } else {
                        illegal_arg("Property does not support this filter.")
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! primitive_filter_between {
    ($name:ident, $data_type:ident, $type:ty, $prop_accessor:ident) => {
        filter_between_struct!($name, $data_type, $type);
        paste! {
            impl Condition for [<$name Cond>] {
                fn evaluate(&self, object: IsarObject, _: Option<&mut FilterCursors>) -> Result<bool> {
                    let val = object.$prop_accessor(self.property);
                    Ok(self.lower <= val && self.upper >= val)
                }

                fn get_linked_collections(&self, _: &mut HashSet<u16>) {}
            }
        }
    };
}

#[macro_export]
macro_rules! float_filter_between {
    ($name:ident, $data_type:ident, $type:ty, $prop_accessor:ident) => {
        filter_between_struct!($name, $data_type, $type);
        paste! {
            impl Condition for [<$name Cond>] {
                fn evaluate(&self, object: IsarObject, _: Option<&mut FilterCursors>) -> Result<bool> {
                    let val = object.$prop_accessor(self.property);
                    let result = if self.upper.is_nan() {
                        self.lower.is_nan() && val.is_nan()
                    } else if self.lower.is_nan() {
                        self.upper >= val || val.is_nan()
                    } else {
                        self.lower <= val && self.upper >= val
                    };
                    Ok(result)
                }

                fn get_linked_collections(&self, _: &mut HashSet<u16>) {}
            }
        }
    };
}

primitive_filter_between!(ByteBetween, Byte, u8, read_byte);
primitive_filter_between!(IntBetween, Int, i32, read_int);
primitive_filter_between!(LongBetween, Long, i64, read_long);
float_filter_between!(FloatBetween, Float, f32, read_float);
float_filter_between!(DoubleBetween, Double, f64, read_double);

#[macro_export]
macro_rules! filter_not_equal_struct {
    ($name:ident, $data_type:ident, $type:ty) => {
        paste! {
            #[derive(Clone)]
            pub struct [<$name Cond>] {
                value: $type,
                property: Property,
            }

            impl [<$name Cond>] {
                pub fn filter(property: Property, value: $type) -> Result<Filter> {
                    if property.data_type == crate::object::data_type::DataType::$data_type {
                        Ok(Filter::$name(Self { property, value }))
                    } else {
                        illegal_arg("Property does not support this filter.")
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! primitive_list_filter {
    ($name:ident, $data_type:ident, $type:ty, $prop_accessor:ident) => {
        filter_not_equal_struct!($name, $data_type, $type);
        paste! {
            impl Condition for [<$name Cond>] {
                fn evaluate(&self, object: IsarObject,_: Option<&mut FilterCursors>) -> Result<bool> {
                    let list = object.$prop_accessor(self.property);
                    if let Some(list) = list {
                        Ok(list.contains(&self.value))
                    } else {
                       Ok( false)
                    }
                }

                fn get_linked_collections(&self, _: &mut HashSet<u16>) {}
            }
        }
    };
}

primitive_list_filter!(ByteListContains, Byte, u8, read_byte_list);
primitive_list_filter!(IntListContains, Int, i32, read_int_list);
primitive_list_filter!(LongListContains, Long, i64, read_long_list);

#[derive(Clone)]
pub struct StringBetweenCond {
    property: Property,
    lower: Option<String>,
    upper: Option<String>,
    case_sensitive: bool,
}

impl StringBetweenCond {
    pub fn filter(
        property: Property,
        lower: Option<&str>,
        upper: Option<&str>,
        case_sensitive: bool,
    ) -> Result<Filter> {
        let lower = if case_sensitive {
            lower.map(|s| s.to_string())
        } else {
            lower.map(|s| s.to_lowercase())
        };
        let upper = if case_sensitive {
            upper.map(|s| s.to_string())
        } else {
            upper.map(|s| s.to_lowercase())
        };
        if property.data_type == crate::object::data_type::DataType::String {
            Ok(Filter::StringBetween(StringBetweenCond {
                property,
                lower,
                upper,
                case_sensitive,
            }))
        } else {
            illegal_arg("Property does not support this filter.")
        }
    }
}

impl Condition for StringBetweenCond {
    fn evaluate(&self, object: IsarObject, _: Option<&mut FilterCursors>) -> Result<bool> {
        let other_str = object.read_string(self.property);
        let result = match (self.lower.as_ref(), other_str) {
            (Some(lower), Some(other)) => {
                if let Some(upper) = self.upper.as_ref() {
                    if self.case_sensitive {
                        lower.as_str() <= other && upper.as_str() >= other
                    } else {
                        let lowercase_other = other.to_lowercase();
                        lower <= &lowercase_other && upper >= &lowercase_other
                    }
                } else {
                    false
                }
            }
            (Some(_), None) => false,
            (None, Some(_)) => true,
            (None, None) => self.upper.is_none(),
        };
        Ok(result)
    }

    fn get_linked_collections(&self, _: &mut HashSet<u16>) {}
}

#[macro_export]
macro_rules! string_filter_struct {
    ($name:ident, $data_type:ident) => {
        paste! {
            #[derive(Clone)]
            pub struct [<$name Cond>] {
                property: Property,
                value: Option<String>,
                case_sensitive: bool,
            }

            impl [<$name Cond>] {
                pub fn filter(
                    property: Property,
                    value: Option<&str>,
                    case_sensitive: bool,
                ) -> Result<Filter> {
                    let value = if case_sensitive {
                        value.map(|s| s.to_string())
                    } else {
                        value.map(|s| s.to_lowercase())
                    };
                    if property.data_type == crate::object::data_type::DataType::$data_type {
                        Ok(Filter::$name([<$name Cond>] {
                            property,
                            value,
                            case_sensitive,
                        }))
                    } else {
                        illegal_arg("Property does not support this filter.")
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! string_filter {
    ($name:ident) => {
        string_filter_struct!($name, String);
        paste! {
            impl Condition for [<$name Cond>] {
                fn evaluate(&self, object: IsarObject, _: Option<&mut FilterCursors>) -> Result<bool> {
                    let other_str = object.read_string(self.property);
                    let result = if let (Some(filter_str), Some(other_str)) = (self.value.as_ref(), other_str) {
                        if self.case_sensitive {
                            string_filter!($name filter_str, other_str)
                        } else {
                            let lowercase_string = other_str.to_lowercase();
                            let lowercase_str = &lowercase_string;
                            string_filter!($name filter_str, lowercase_str)
                        }
                    } else {
                        self.value.is_none() && other_str.is_none()
                    };
                    Ok(result)
                }

                fn get_linked_collections(&self, _: &mut HashSet<u16>) {}
            }
        }
    };

    (StringStartsWith $filter_str:ident, $other_str:ident) => {
        $other_str.starts_with($filter_str)
    };

    (StringEndsWith $filter_str:ident, $other_str:ident) => {
        $other_str.ends_with($filter_str)
    };

    (StringMatches $filter_str:ident, $other_str:ident) => {
        fast_wild_match($other_str, $filter_str)
    };
}

string_filter!(StringStartsWith);
string_filter!(StringEndsWith);
string_filter!(StringMatches);

string_filter_struct!(StringListContains, StringList);

impl Condition for StringListContainsCond {
    fn evaluate(&self, object: IsarObject, _: Option<&mut FilterCursors>) -> Result<bool> {
        let list = object.read_string_list(self.property);
        if let Some(list) = list {
            Ok(list.contains(&self.value.as_deref()))
        } else {
            Ok(false)
        }
    }

    fn get_linked_collections(&self, _: &mut HashSet<u16>) {}
}

#[derive(Clone)]
pub struct AndCond {
    filters: Vec<Filter>,
}

impl Condition for AndCond {
    fn evaluate(
        &self,
        object: IsarObject,
        mut cursors: Option<&mut FilterCursors>,
    ) -> Result<bool> {
        for filter in &self.filters {
            if !filter.evaluate(object, cursors.as_deref_mut())? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn get_linked_collections(&self, col_ids: &mut HashSet<u16>) {
        for filter in &self.filters {
            filter.get_linked_collections(col_ids);
        }
    }
}

impl AndCond {
    pub fn filter(filters: Vec<Filter>) -> Filter {
        Filter::And(AndCond { filters })
    }
}

#[derive(Clone)]
pub struct OrCond {
    filters: Vec<Filter>,
}

impl Condition for OrCond {
    fn evaluate(
        &self,
        object: IsarObject,
        mut cursors: Option<&mut FilterCursors>,
    ) -> Result<bool> {
        for filter in &self.filters {
            if filter.evaluate(object, cursors.as_deref_mut())? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn get_linked_collections(&self, col_ids: &mut HashSet<u16>) {
        for filter in &self.filters {
            filter.get_linked_collections(col_ids);
        }
    }
}

impl OrCond {
    pub fn filter(filters: Vec<Filter>) -> Filter {
        Filter::Or(OrCond { filters })
    }
}

#[derive(Clone)]
pub struct NotCond {
    filter: Box<Filter>,
}

impl Condition for NotCond {
    fn evaluate(&self, object: IsarObject, cursors: Option<&mut FilterCursors>) -> Result<bool> {
        Ok(!self.filter.evaluate(object, cursors)?)
    }

    fn get_linked_collections(&self, col_ids: &mut HashSet<u16>) {
        self.filter.get_linked_collections(col_ids);
    }
}

impl NotCond {
    pub fn filter(filter: Filter) -> Filter {
        Filter::Not(NotCond {
            filter: Box::new(filter),
        })
    }
}

#[derive(Clone)]
pub struct StaticCond {
    value: bool,
}

impl Condition for StaticCond {
    fn evaluate(&self, _: IsarObject, _: Option<&mut FilterCursors>) -> Result<bool> {
        Ok(self.value)
    }

    fn get_linked_collections(&self, _: &mut HashSet<u16>) {}
}

impl StaticCond {
    pub fn filter(value: bool) -> Filter {
        Filter::Static(StaticCond { value })
    }
}

#[derive(Clone)]
pub struct LinkCond {
    link: Link,
    id_property: Property,
    filter: Box<Filter>,
}

impl Condition for LinkCond {
    fn evaluate(&self, object: IsarObject, cursors: Option<&mut FilterCursors>) -> Result<bool> {
        let oid = object.read_long(self.id_property);
        if let Some(cursors) = cursors {
            self.link
                .iter(cursors.0, cursors.1, oid, |object| {
                    self.filter.evaluate(object, None).map(|matches| !matches)
                })
                .map(|none_matches| !none_matches)
        } else {
            Err(IsarError::VersionError {})
        }
    }

    fn get_linked_collections(&self, col_ids: &mut HashSet<u16>) {
        col_ids.insert(self.link.get_target_col_id());
    }
}

impl LinkCond {
    pub fn filter(
        collection: &IsarCollection,
        target_collection: &IsarCollection,
        link_index: usize,
        backlink: bool,
        filter: Filter,
    ) -> Result<Filter> {
        let link = collection.get_link_backlink(link_index, backlink)?;
        let id_property = target_collection.get_oid_property();
        Ok(Filter::Link(LinkCond {
            link,
            id_property,
            filter: Box::new(filter),
        }))
    }
}
