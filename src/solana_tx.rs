use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::Instruction,
    message::Message,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

/// Creates a dummy transaction on the Solana blockchain
pub fn create_dummy_transaction() -> Result<Transaction, Box<dyn std::error::Error>> {
    // Connect to Solana devnet
    let client = RpcClient::new("https://api.devnet.solana.com");

    // Generate a dummy keypair
    let payer = Keypair::new();
    println!("Generated Payer Address: {}", payer.pubkey());

    // Define the recipient of the transaction
    let recipient = Pubkey::new_unique();
    println!("Recipient Address: {}", recipient);

    // Create a transfer instruction
    let transfer_instruction = solana_sdk::system_instruction::transfer(
        &payer.pubkey(),
        &recipient,
        1_000_000, // 0.001 SOL
    );

    // Build the message and transaction
    let message = Message::new(&[transfer_instruction], Some(&payer.pubkey()));
    let recent_blockhash = client.get_latest_blockhash()?;
    let transaction = Transaction::new(&[&payer], message, recent_blockhash);

    Ok(transaction)
}
