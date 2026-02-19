use crate::models::ParseResult;

pub trait Transformation {
    fn transform(&self, result: &mut ParseResult);
}
