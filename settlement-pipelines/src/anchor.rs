use anchor_client::{DynSigner, RequestBuilder};
use anyhow::anyhow;
use log::error;
use solana_transaction_builder::TransactionBuilder;
use std::rc::Rc;

pub fn add_instructions_to_builder_from_anchor(
    transaction_builder: &mut TransactionBuilder,
    request_builder: &RequestBuilder<Rc<DynSigner>>,
) -> anyhow::Result<()> {
    add_instructions_to_builder_from_anchor_internal(transaction_builder, request_builder, None)
}

pub fn add_instruction_to_builder_from_anchor_with_description(
    transaction_builder: &mut TransactionBuilder,
    request_builder: &RequestBuilder<Rc<DynSigner>>,
    description: String,
) -> anyhow::Result<()> {
    add_instructions_to_builder_from_anchor_internal(
        transaction_builder,
        request_builder,
        Some(vec![description]),
    )
}

fn add_instructions_to_builder_from_anchor_internal(
    transaction_builder: &mut TransactionBuilder,
    request_builder: &RequestBuilder<Rc<DynSigner>>,
    descriptions: Option<Vec<String>>,
) -> anyhow::Result<()> {
    let instructions = request_builder.instructions().map_err(|e| {
        error!(
            "add_instructions_from_anchor_builder: error building instructions: {:?}",
            e
        );
        anyhow!(e)
    })?;
    if let Some(descriptions) = descriptions {
        if instructions.len() != descriptions.len() {
            return Err(anyhow!("add_instructions_from_anchor_builder: instructions and descriptions must have the same length"));
        }
        transaction_builder.add_instructions_with_description(
            instructions.into_iter().zip(descriptions.into_iter()),
        )?;
    } else {
        transaction_builder.add_instructions(instructions)?;
    }
    transaction_builder.finish_instruction_pack();
    Ok(())
}
