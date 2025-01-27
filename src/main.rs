use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::Instruction,
    message::Message,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use std::error::Error;

use libp2p::{
    core::upgrade,
    gossipsub::{Gossipsub, GossipsubConfig, GossipsubEvent, IdentTopic, MessageAuthenticity},
    identity,
    mdns::{Mdns, MdnsConfig, MdnsEvent},
    noise::{Keypair as NoiseKeypair, NoiseConfig, X25519Spec},
    swarm::{Swarm, SwarmBuilder, SwarmEvent},
    tcp::TokioTcpConfig,
    yamux::YamuxConfig,
    Multiaddr, NetworkBehaviour, PeerId, Transport,
};

mod solana_tx;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create and print a dummy transaction
    match solana_tx::create_dummy_transaction().await {
        Ok(tx) => {
            println!("Dummy Transaction Created: {:?}", tx);
        }
        Err(err) => {
            eprintln!("Failed to create transaction: {}", err);
        }
    }

    // Generate an identity keypair for the node
    let id_keys = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(id_keys.public());
    println!("Local peer id: {:?}", peer_id);

    // Create Gossipsub configuration
    let gossipsub_config = GossipsubConfig::default();
    let mut gossipsub = Gossipsub::new(
        MessageAuthenticity::Signed(id_keys.clone()),
        gossipsub_config,
    )?;

    // Subscribe to the "transaction" topic
    let topic = IdentTopic::new("transaction");
    gossipsub.subscribe(&topic)?;

    // Create an mDNS service for local peer discovery
    let mdns = Mdns::new(MdnsConfig::default()).await?;

    // Combine behaviours
    #[derive(NetworkBehaviour)]
    struct MyBehaviour {
        gossipsub: Gossipsub,
        mdns: Mdns,
    }

    let behaviour = MyBehaviour { gossipsub, mdns };

    // Set up the transport
    let noise_keys = NoiseKeypair::<X25519Spec>::new().into_authentic(&id_keys)?;
    let transport = TokioTcpConfig::new()
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(noise_keys).into_authenticated())
        .multiplex(YamuxConfig::default())
        .boxed();

    // Create the swarm
    let mut swarm = SwarmBuilder::new(transport, behaviour, peer_id)
        .executor(Box::new(|fut| {
            tokio::spawn(fut);
        }))
        .build();

    // Listen on a random port
    let addr: Multiaddr = "/ip4/0.0.0.0/tcp/0".parse()?;
    Swarm::listen_on(&mut swarm, addr)?;

    // Event loop
    loop {
        match swarm.select_next_some().await {
            SwarmEvent::Behaviour(libp2p::gossipsub::GossipsubEvent::Message {
                propagation_source,
                message_id,
                message,
            }) => {
                println!(
                    "Received message: {:?} with ID: {:?} from peer: {:?}",
                    String::from_utf8_lossy(&message.data),
                    message_id,
                    propagation_source
                );
            }
            SwarmEvent::Behaviour(MdnsEvent::Discovered(peers)) => {
                for (peer, _addr) in peers {
                    swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer);
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {:?}", address);
            }
            _ => {}
        }
    }
}

/// Solana Transaction Helper Module
pub mod solana_tx {
    use solana_client::rpc_client::RpcClient;
    use solana_sdk::{
        instruction::Instruction,
        message::Message,
        pubkey::Pubkey,
        signature::{Keypair, Signer},
        system_instruction,
        transaction::Transaction,
    };
    use std::error::Error;

    /// Creates a dummy transaction on the Solana blockchain
    pub async fn create_dummy_transaction() -> Result<Transaction, Box<dyn Error>> {
        // Connect to Solana devnet
        let client = RpcClient::new("https://api.devnet.solana.com");

        // Generate a dummy keypair (payer)
        let payer = Keypair::new();
        println!("Generated Payer Address: {}", payer.pubkey());

        // Define the recipient address (for simplicity, using a random public key)
        let recipient = Pubkey::new_unique();
        println!("Recipient Address: {}", recipient);

        // Create a transfer instruction
        let transfer_instruction = system_instruction::transfer(
            &payer.pubkey(),
            &recipient,
            1_000_000, // Transfer 0.001 SOL
        );

        // Create a message from the transfer instruction
        let message = Message::new(&[transfer_instruction], Some(&payer.pubkey()));

        // Get the latest blockhash
        let recent_blockhash = client.get_latest_blockhash()?;

        // Build the transaction with the payer and the recent blockhash
        let transaction = Transaction::new(&[&payer], message, recent_blockhash);

        Ok(transaction)
    }
}
