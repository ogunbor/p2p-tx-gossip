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
use serde::{Deserialize, Serialize};
use serde_json;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::Instruction,
    message::Message,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use std::error::Error; // For serializing and deserializing transactions

mod solana_tx;

// Struct to track peer information (e.g., peer_id, number of transactions)
#[derive(Serialize, Deserialize, Debug)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub transactions_sent: usize,
}

// Struct to track transactions
#[derive(Serialize, Deserialize, Debug)]
pub struct TransactionMessage {
    pub peer_id: PeerId,
    pub transaction: Transaction,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Generate an identity keypair for the node
    let id_keys = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(id_keys.public());
    println!("Local peer id: {:?}", peer_id);

    // Create a dummy transaction
    let tx = solana_tx::create_dummy_transaction().await?;
    println!("Dummy Transaction Created: {:?}", tx);

    // Create a Gossipsub configuration
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

    // Create distributed tables for peers and transactions
    let mut peers_table = std::collections::HashMap::new();
    let mut transactions_table = std::collections::HashMap::new();

    // Event loop
    loop {
        match swarm.select_next_some().await {
            SwarmEvent::Behaviour(libp2p::gossipsub::GossipsubEvent::Message {
                propagation_source,
                message_id,
                message,
            }) => {
                // Deserialize the received message into a TransactionMessage
                let received_msg: TransactionMessage = match serde_json::from_slice(&message.data) {
                    Ok(msg) => msg,
                    Err(_) => {
                        println!("Failed to deserialize message");
                        continue;
                    }
                };

                // Update the transactions table with the received transaction
                transactions_table.insert(received_msg.peer_id, received_msg.transaction.clone());

                // Update the peers table (this can include additional peer metadata)
                if let Some(peer_info) = peers_table.get_mut(&received_msg.peer_id) {
                    peer_info.transactions_sent += 1;
                } else {
                    peers_table.insert(
                        received_msg.peer_id.clone(),
                        PeerInfo {
                            peer_id: received_msg.peer_id,
                            transactions_sent: 1,
                        },
                    );
                }

                // Print updated peer and transaction tables
                println!("Updated Peers Table: {:?}", peers_table);
                println!("Updated Transactions Table: {:?}", transactions_table);
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

        // Send a transaction across peers
        let tx_message = TransactionMessage {
            peer_id: peer_id.clone(),
            transaction: tx.clone(),
        };
        let serialized_message = serde_json::to_vec(&tx_message)?;

        gossipsub.publish(&topic, serialized_message)?;

        // Add some delay to prevent sending transactions too frequently
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}
