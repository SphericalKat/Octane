use crate::path::PathNode;
use crate::request::RequestMethod;
use crate::router::Closure;
use std::collections::HashMap;

pub type Paths = HashMap<RequestMethod, PathNode<Closures>>;

pub struct Closures {
    pub closure: Closure,
    pub index: usize,
}

#[macro_export]
macro_rules! inject_method {
    ( $instance: expr, $path: expr, $closure: expr, $method: expr ) => {
        use crate::middlewares::Closures;
        if let Some(paths) = $instance.paths.get_mut($method) {
            paths.insert(
                PathBuf::parse($path)?,
                Closures {
                    closure: $closure,
                    index: $instance.route_counter + 1,
                },
            );
        }
    };
}