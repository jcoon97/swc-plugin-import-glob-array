use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::str::FromStr;

use is_glob::is_glob;
use swc_core::ecma::ast::{Decl, ImportDecl, Module, ModuleDecl, ModuleItem, Stmt, VarDecl};
use swc_core::ecma::visit::Fold;
use swc_core::ecma::{ast::Program, visit::FoldWith};
use swc_core::plugin::metadata::TransformPluginMetadataContextKind;
use swc_core::plugin::{plugin_transform, proxies::TransformPluginProgramMetadata};

use crate::transformer::transform_import_decl;

mod transformer;
mod utils;

const IMPORT_META_NAME: &'static str = "_importMeta";

#[derive(Debug)]
struct ImportGlobArrayPlugin {
    cwd: PathBuf,
    filename: PathBuf,
    id_counter: Rc<RefCell<usize>>,
}

#[derive(Debug)]
struct ImportPaths {
    absolute_path: String,
    imported_path: String,
}

impl ImportGlobArrayPlugin {
    fn as_glob_path(&self, filename: &str) -> PathBuf {
        self.cwd.join(&self.filename).with_file_name(filename)
    }

    fn build_module_items(
        &self,
        tuple: Option<(Vec<ImportDecl>, Vec<VarDecl>, Vec<VarDecl>)>,
    ) -> Vec<ModuleItem> {
        let mut results: Vec<ModuleItem> = vec![];

        if let Some(transformed) = tuple {
            transformed
                .0
                .into_iter()
                .for_each(|item| results.push(ModuleItem::ModuleDecl(ModuleDecl::Import(item))));

            transformed.1.into_iter().for_each(|item| {
                results.push(ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(item)))))
            });

            transformed.2.into_iter().for_each(|item| {
                results.push(ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(item)))))
            });
        }
        results
    }

    fn get_paths(&self, path: &PathBuf) -> Option<ImportPaths> {
        let current_dir = self.cwd.join(self.filename.parent().unwrap());
        let relative_path = path.strip_prefix(&current_dir).ok()?.to_str()?.to_owned();
        let absolute_path = current_dir.join(&relative_path).to_str()?.to_owned();
        let imported_path = if relative_path.starts_with('.') {
            relative_path.to_owned()
        } else {
            format!("./{relative_path}")
        };
        Some(ImportPaths {
            absolute_path,
            imported_path,
        })
    }

    fn next_id(&self, starting_id: &str) -> String {
        *self.id_counter.borrow_mut() = self.id_counter.take() + 1;
        format!("{}{}", starting_id, self.id_counter.borrow())
    }

    fn new(cwd: PathBuf, filename: PathBuf) -> impl Fold {
        Self {
            cwd,
            filename,
            id_counter: Rc::new(RefCell::new(0)),
        }
    }
}

impl Fold for ImportGlobArrayPlugin {
    fn fold_module(&mut self, mut module: Module) -> Module {
        module.body = module
            .body
            .into_iter()
            .flat_map(|item| match item {
                ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl))
                    if (import_decl.src.value.starts_with('.')
                        || import_decl.src.value.starts_with('/'))
                        && is_glob(&import_decl.src.value.to_string()) =>
                {
                    self.build_module_items(transform_import_decl(&self, &import_decl))
                }
                _ => vec![item],
            })
            .collect();
        module
    }
}

#[plugin_transform]
pub fn process_transform(program: Program, metadata: TransformPluginProgramMetadata) -> Program {
    let file_name = metadata
        .get_context(&TransformPluginMetadataContextKind::Filename)
        .map(PathBuf::from)
        .expect("Import Glob Array Plugin requires filename metadata");
    let cwd = PathBuf::from_str("/cwd").unwrap();
    let mut plugin = ImportGlobArrayPlugin::new(cwd, file_name);
    program.fold_with(&mut plugin)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use swc_core::ecma::transforms::testing::{test_fixture, FixtureTestConfig};
    use swc_core::testing::fixture;

    use crate::ImportGlobArrayPlugin;

    #[fixture("tests/fixtures/**/input.js")]
    fn fixture(input: PathBuf) {
        let cwd = input.parent().unwrap().to_path_buf();
        let output = input.with_file_name("output.js");

        test_fixture(
            Default::default(),
            &|_| ImportGlobArrayPlugin::new(cwd.clone(), input.clone()),
            &input,
            &output,
            FixtureTestConfig {
                allow_error: false,
                sourcemap: false,
            },
        )
    }
}
