use super::ScriptEditorView;
use forge_components::{ActivePresentation, FONT_XS, InlineEdit, InlineEditEvent, inline_edit, tr};
use forge_script::is_engine_bound_name;
use forge_types::{ScriptContract, ScriptInput, VariantKind};
use gpui::{Context, Entity, Pixels, Point, Subscription, Window};

pub(super) struct InputRename {
    pub(super) index: usize,
    pub(super) editor: Entity<InlineEdit>,
    pub(super) _sub: Subscription,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum KindTarget {
    Input(usize),
    Returns,
}

pub(super) struct KindMenu {
    pub(super) target: KindTarget,
    pub(super) position: Point<Pixels>,
}

enum InputNameError {
    Empty,
    NotIdentifier,
    Reserved,
    Duplicate,
}

impl InputNameError {
    pub(super) fn message(&self, name: &str) -> String {
        match self {
            InputNameError::Empty => tr!("script_editor_input_name_empty"),
            InputNameError::NotIdentifier => tr!("script_editor_input_name_invalid", name = name),
            InputNameError::Reserved => tr!("script_editor_input_name_reserved", name = name),
            InputNameError::Duplicate => tr!("script_editor_input_name_duplicate", name = name),
        }
    }
}

fn is_rhai_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn validate_input_name(
    name: &str,
    index: usize,
    inputs: &[ScriptInput],
) -> Result<(), InputNameError> {
    if name.is_empty() {
        return Err(InputNameError::Empty);
    }
    if !is_rhai_identifier(name) {
        return Err(InputNameError::NotIdentifier);
    }
    if is_engine_bound_name(name) {
        return Err(InputNameError::Reserved);
    }
    if inputs
        .iter()
        .enumerate()
        .any(|(i, input)| i != index && input.name == name)
    {
        return Err(InputNameError::Duplicate);
    }
    Ok(())
}

fn next_input_name(inputs: &[ScriptInput]) -> String {
    let mut ordinal = 1usize;
    loop {
        let candidate = format!("input_{ordinal}");
        if inputs.iter().all(|input| input.name != candidate) {
            return candidate;
        }
        ordinal += 1;
    }
}

impl ScriptEditorView {
    pub(super) fn reset_contract_editing(&mut self) {
        self.input_rename = None;
        self.kind_menu = None;
        self.contract_error = None;
    }

    pub(super) fn contract(&self) -> Option<&ScriptContract> {
        self.open.as_ref().map(|open| &open.record.contract)
    }

    fn contract_mut(&mut self) -> Option<&mut ScriptContract> {
        self.open.as_mut().map(|open| &mut open.record.contract)
    }

    pub(super) fn add_contract_input(&mut self, cx: &mut Context<Self>) {
        let Some(contract) = self.contract_mut() else {
            return;
        };
        let name = next_input_name(&contract.inputs);
        contract.inputs.push(ScriptInput {
            name,
            kind: VariantKind::String,
        });
        self.reset_contract_editing();
        cx.notify();
    }

    pub(super) fn remove_contract_input(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(contract) = self.contract_mut() else {
            return;
        };
        if index >= contract.inputs.len() {
            return;
        }
        contract.inputs.remove(index);
        self.reset_contract_editing();
        cx.notify();
    }

    pub(super) fn move_contract_input(&mut self, index: usize, up: bool, cx: &mut Context<Self>) {
        let Some(contract) = self.contract_mut() else {
            return;
        };
        let target = if up {
            index.checked_sub(1)
        } else {
            index.checked_add(1)
        };
        let Some(target) = target.filter(|t| *t < contract.inputs.len()) else {
            return;
        };
        contract.inputs.swap(index, target);
        self.reset_contract_editing();
        cx.notify();
    }

    pub(super) fn open_kind_menu(
        &mut self,
        target: KindTarget,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.row_menu = None;
        self.kind_menu = Some(KindMenu { target, position });
        cx.notify();
    }

    pub(super) fn close_kind_menu(&mut self, cx: &mut Context<Self>) {
        self.kind_menu = None;
        cx.notify();
    }

    pub(super) fn set_contract_kind(
        &mut self,
        target: KindTarget,
        kind: VariantKind,
        cx: &mut Context<Self>,
    ) {
        let Some(contract) = self.contract_mut() else {
            return;
        };
        match target {
            KindTarget::Input(index) => {
                let Some(input) = contract.inputs.get_mut(index) else {
                    return;
                };
                input.kind = kind;
            }
            KindTarget::Returns => contract.returns = Some(kind),
        }
        self.kind_menu = None;
        self.contract_error = None;
        cx.notify();
    }

    pub(super) fn clear_contract_return(&mut self, cx: &mut Context<Self>) {
        let Some(contract) = self.contract_mut() else {
            return;
        };
        contract.returns = None;
        self.kind_menu = None;
        self.contract_error = None;
        cx.notify();
    }

    pub(super) fn start_input_rename(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(current) = self
            .contract()
            .and_then(|contract| contract.inputs.get(index))
            .map(|input| input.name.clone())
        else {
            return;
        };
        self.kind_menu = None;
        self.contract_error = None;
        self.open_input_editor(index, current, window, cx);
        cx.notify();
    }

    fn open_input_editor(
        &mut self,
        index: usize,
        seed: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let palette = cx.palette();
        let editor = inline_edit(seed, palette, FONT_XS, window, cx);
        let sub = cx.subscribe_in(
            &editor,
            window,
            |this, _e, event: &InlineEditEvent, window, cx| match event {
                InlineEditEvent::Commit(next) => this.commit_input_rename(next.clone(), window, cx),
                InlineEditEvent::Cancel => this.cancel_input_rename(cx),
            },
        );
        self.input_rename = Some(InputRename {
            index,
            editor,
            _sub: sub,
        });
    }

    fn commit_input_rename(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.input_rename.as_ref().map(|state| state.index) else {
            return;
        };
        let name = name.trim().to_owned();
        let Some(contract) = self.contract() else {
            self.input_rename = None;
            cx.notify();
            return;
        };
        let unchanged = contract
            .inputs
            .get(index)
            .is_some_and(|input| input.name == name);
        let outcome = if unchanged {
            Ok(())
        } else {
            validate_input_name(&name, index, &contract.inputs)
        };

        match outcome {
            Ok(()) => {
                if let Some(contract) = self.contract_mut()
                    && let Some(input) = contract.inputs.get_mut(index)
                {
                    input.name = name;
                }
                self.input_rename = None;
                self.contract_error = None;
            }
            Err(error) => {
                self.contract_error = Some(error.message(&name).into());
                self.open_input_editor(index, name, window, cx);
            }
        }
        cx.notify();
    }

    fn cancel_input_rename(&mut self, cx: &mut Context<Self>) {
        self.input_rename = None;
        cx.notify();
    }
}
