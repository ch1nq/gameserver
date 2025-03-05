#[derive(Debug, Clone)]
pub enum AgentStatus {
    Active,
    Inactive,
}

#[derive(Debug, Clone)]
pub struct AgentStats {
    pub wins: u32,
    pub losses: u32,
    pub rank: u32,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub name: String,
    pub status: AgentStatus,
    pub stats: AgentStats,
}

pub fn get_agents() -> Vec<Agent> {
    vec![
        Agent {
            name: "Alice".to_string(),
            status: AgentStatus::Active,
            stats: AgentStats {
                wins: 10,
                losses: 5,
                rank: 1,
            },
        },
        Agent {
            name: "Bob".to_string(),
            status: AgentStatus::Inactive,
            stats: AgentStats {
                wins: 5,
                losses: 10,
                rank: 2,
            },
        },
        Agent {
            name: "Charlie".to_string(),
            status: AgentStatus::Active,
            stats: AgentStats {
                wins: 7,
                losses: 7,
                rank: 3,
            },
        },
        Agent {
            name: "David".to_string(),
            status: AgentStatus::Active,
            stats: AgentStats {
                wins: 6,
                losses: 8,
                rank: 4,
            },
        },
        Agent {
            name: "Eve".to_string(),
            status: AgentStatus::Inactive,
            stats: AgentStats {
                wins: 4,
                losses: 11,
                rank: 5,
            },
        },
        Agent {
            name: "Frank".to_string(),
            status: AgentStatus::Active,
            stats: AgentStats {
                wins: 8,
                losses: 6,
                rank: 6,
            },
        },
        Agent {
            name: "Grace".to_string(),
            status: AgentStatus::Active,
            stats: AgentStats {
                wins: 9,
                losses: 5,
                rank: 7,
            },
        },
    ]
}
