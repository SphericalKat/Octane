use crate::path::{InvalidPathError, PathBuf, PathNode};
use crate::request::{Request, RequestMethod};
use crate::responder::Response;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::ops::Deref;
use std::result::Result;

pub type Closure =
    Box<dyn for<'a> Fn(&'a Request, &'a mut Response) -> BoxFuture<'a, ()> + Send + Sync>;
pub type ClosureFlow =
    Box<dyn for<'a> Fn(&'a Request, &'a mut Response) -> BoxFuture<'a, Flow> + Send + Sync>;
pub type RouterResult = Result<(), InvalidPathError>;
pub type Paths = HashMap<RequestMethod, PathNode<ClosureCounter>>;

pub enum Flow {
    Stop,
    Continue,
}

pub struct ClosureCounter {
    pub closure: Closure,
    pub index: usize,
}

pub trait Route {
    fn get(&mut self, path: &str, closure: Closure) -> RouterResult;
    fn post(&mut self, path: &str, closure: Closure) -> RouterResult;
    fn all(&mut self, path: &str, closure: Closure) -> RouterResult;
    fn add(&mut self, entity: ClosureFlow) -> RouterResult;
    fn add_route(&mut self, path: &str, closure: Closure) -> RouterResult;
}

pub struct Router {
    pub paths: Paths,
    pub route_counter: usize,
}

impl Deref for Router {
    type Target = Paths;

    fn deref(&self) -> &Self::Target {
        &self.paths
    }
}

impl Router {
    pub fn new() -> Self {
        let mut paths: Paths = HashMap::new();
        for variants in RequestMethod::values().iter().cloned() {
            paths.insert(variants, PathNode::new());
        }
        Router {
            paths,
            route_counter: 0,
        }
    }
    pub fn append(&mut self, _router: Self) -> &mut Self {
        // TODO: Append each of the routes with respective keys
        self
    }
}

impl Route for Router {
    fn get(&mut self, path: &str, closure: Closure) -> RouterResult {
        if let Some(paths) = self.paths.get_mut(&RequestMethod::Get) {
            paths.insert(
                PathBuf::parse(path)?,
                ClosureCounter {
                    closure,
                    index: self.route_counter + 1,
                },
            );
        }
        Ok(())
    }
    fn post(&mut self, path: &str, closure: Closure) -> RouterResult {
        if let Some(paths) = self.paths.get_mut(&RequestMethod::Post) {
            paths.insert(
                PathBuf::parse(path)?,
                ClosureCounter {
                    closure,
                    index: self.route_counter + 1,
                },
            );
        }
        Ok(())
    }
    fn all(&mut self, path: &str, closure: Closure) -> RouterResult {
        if let Some(paths) = self.paths.get_mut(&RequestMethod::Post) {
            paths.insert(
                PathBuf::parse(path)?,
                ClosureCounter {
                    closure,
                    index: self.route_counter + 1,
                },
            );
        }
        Ok(())
    }
    fn add(&mut self, _entity: ClosureFlow) -> RouterResult {
        Ok(())
    }
    fn add_route(&mut self, _path: &str, _closure: Closure) -> RouterResult {
        Ok(())
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

#[macro_export]
macro_rules! route {
    ( | $req : ident, $res : ident | $body : expr ) => {
        #[allow(unused_variables)]
        Box::new(move |$req, $res| Box::pin(async move { $body }))
    };
}
