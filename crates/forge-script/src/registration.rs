use std::sync::Arc;

use rhai::{Dynamic, INT, ImmutableString, Map, Module, RhaiNativeFunc};
use time::OffsetDateTime;

use crate::catalog::{MethodDescriptor, ParamDescriptor};

pub(crate) trait ScriptType {
    const NAME: &'static str;
}

impl ScriptType for () {
    const NAME: &'static str = "()";
}

impl ScriptType for INT {
    const NAME: &'static str = "int";
}

impl ScriptType for bool {
    const NAME: &'static str = "bool";
}

impl ScriptType for ImmutableString {
    const NAME: &'static str = "string";
}

impl ScriptType for OffsetDateTime {
    const NAME: &'static str = "datetime";
}

impl ScriptType for Map {
    const NAME: &'static str = "Map";
}

impl ScriptType for Dynamic {
    const NAME: &'static str = "Variant";
}

pub(crate) trait ScriptParams<const N: usize> {
    const TYPES: [&'static str; N];
}

impl ScriptParams<0> for () {
    const TYPES: [&'static str; 0] = [];
}

macro_rules! script_params {
    ($n:literal: $($param:ident),+) => {
        impl<$($param: ScriptType),+> ScriptParams<$n> for ($($param,)+) {
            const TYPES: [&'static str; $n] = [$($param::NAME),+];
        }
    };
}

script_params!(1: A);
script_params!(2: A, B);
script_params!(3: A, B, C);
script_params!(4: A, B, C, D);

pub(crate) struct ScriptFn<const N: usize> {
    pub name: &'static str,
    pub params: [&'static str; N],
    pub doc: Option<&'static str>,
}

#[derive(Default)]
pub(crate) struct ApiModule {
    module: Module,
    methods: Vec<MethodDescriptor>,
}

impl ApiModule {
    pub(crate) fn register<A, const N: usize, R, F>(&mut self, spec: ScriptFn<N>, func: F)
    where
        A: ScriptParams<N> + 'static,
        R: ScriptType + Clone + Send + Sync + 'static,
        F: RhaiNativeFunc<A, N, false, R, true> + Send + Sync + 'static,
    {
        self.module.set_native_fn(spec.name, func);
        self.methods.push(MethodDescriptor {
            namespace: None,
            name: spec.name,
            params: spec
                .params
                .into_iter()
                .zip(A::TYPES)
                .map(|(name, ty)| ParamDescriptor { name, ty })
                .collect(),
            return_type: R::NAME,
            doc: spec.doc,
        });
    }

    pub(crate) fn mount(&mut self, namespace: &'static str, child: ApiModule) {
        self.module.set_sub_module(namespace, child.module);
        self.methods
            .extend(child.methods.into_iter().map(|method| MethodDescriptor {
                namespace: Some(namespace),
                ..method
            }));
    }

    pub(crate) fn into_module(self) -> Arc<Module> {
        Arc::new(self.module)
    }

    pub(crate) fn into_methods(self) -> Vec<MethodDescriptor> {
        self.methods
    }
}
