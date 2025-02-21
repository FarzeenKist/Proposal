use candid::{CandidType, Decode, Deserialize, Encode};
use ic_cdk::{caller, query, update};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    {BoundedStorable, DefaultMemoryImpl, StableBTreeMap, Storable},
};
use std::{borrow::Cow, cell::RefCell};

type Memory = VirtualMemory<DefaultMemoryImpl>;
const MAX_VALUE_SIZE: u32 = 5000;

#[derive(CandidType, Deserialize)]
enum Choice {
    Approve,
    Reject,
    Pass,
}

#[derive(CandidType)]
enum VoteError {
    AlreadyVoted,
    ProposalIsNotActive,
    NoSuchProposal,
    AccessRejected,
    UpdateError(String), // Improved error message
}

#[derive(CandidType, Deserialize)]
struct Proposal {
    description: String,
    approve: u32,
    reject: u32,
    pass: u32,
    is_active: bool,
    voted: Vec<candid::Principal>,
    owner: candid::Principal,
}

#[derive(CandidType, Deserialize)]
struct CreateProposal {
    description: String,
    is_active: bool,
}

impl Storable for Proposal {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

impl BoundedStorable for Proposal {
    const MAX_SIZE: u32 = MAX_VALUE_SIZE;
    const IS_FIXED_SIZE: bool = false;
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static PROPOSAL_MAP: RefCell<StableBTreeMap<u64, Proposal, Memory>> = RefCell::new(
        StableBTreeMap::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))))
    );
}

#[query]
fn get_proposal(key: u64) -> Option<Proposal> {
    PROPOSAL_MAP.with(|p| p.borrow().get(&key))
}

#[query]
fn get_proposal_count() -> u64 {
    PROPOSAL_MAP.with(|p| p.borrow().len())
}

#[update]
fn create_proposal(key: u64, proposal: CreateProposal) -> Option<Proposal> {
    let value: Proposal = Proposal {
        description: proposal.description,
        approve: 0u32,
        reject: 0u32,
        pass: 0u32,
        is_active: proposal.is_active,
        voted: vec![],
        owner: caller(),
    };

    PROPOSAL_MAP.with(|p| p.borrow_mut().insert(key, value))
}

#[update]
fn edit_proposal(key: u64, proposal: CreateProposal) -> Result<(), VoteError> {
    PROPOSAL_MAP.with(|p| {
        let old_proposal_opt: Option<Proposal> = p.borrow().get(&key);
        let old_proposal = old_proposal_opt.ok_or(VoteError::NoSuchProposal)?;

        if caller() != old_proposal.owner {
            return Err(VoteError::AccessRejected);
        };

        let value: Proposal = Proposal {
            description: proposal.description,
            is_active: proposal.is_active,
            ..old_proposal
        };

        p.borrow_mut().insert(key, value).ok_or(VoteError::UpdateError("Insert failed".to_string()))
    })
}

#[update]
fn end_proposal(key: u64) -> Result<(), VoteError> {
    PROPOSAL_MAP.with(|p| {
        let proposal_opt: Option<Proposal> = p.borrow().get(&key);
        let mut proposal = proposal_opt.ok_or(VoteError::NoSuchProposal)?;

        if caller() != proposal.owner {
            return Err(VoteError::AccessRejected);
        };

        proposal.is_active = false;

        p.borrow_mut().insert(key, proposal).ok_or(VoteError::UpdateError("Insert failed".to_string()))
    })
}

#[update]
fn vote(key: u64, choice: Choice) -> Result<(), VoteError> {
    PROPOSAL_MAP.with(|p| {
        let proposal_opt: Option<Proposal> = p.borrow().get(&key);
        let mut proposal = proposal_opt.ok_or(VoteError::NoSuchProposal)?;

        let caller = caller();

        if proposal.voted.contains(&caller) {
            return Err(VoteError::AlreadyVoted);
        } else if !proposal.is_active {
            return Err(VoteError::ProposalIsNotActive);
        };

        match choice {
            Choice::Approve => proposal.approve += 1,
            Choice::Reject => proposal.reject += 1,
            Choice::Pass => proposal.pass += 1,
        };

        proposal.voted.push(caller);

        p.borrow_mut().insert(key, proposal).ok_or(VoteError::UpdateError("Insert failed".to_string()))
    })
}
