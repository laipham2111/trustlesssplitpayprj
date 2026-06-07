#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    token, Address, Env, String, Vec,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    Pending,
    PartiallyPaid,
    Completed,
    Cancelled,
}

#[contracttype]
#[derive(Clone)]
pub struct Bill {
    pub id: u64,
    pub creator: Address,
    pub token: Address,
    pub title: String,
    pub total: i128,
    pub participants: Vec<Address>,
    pub shares: Vec<i128>,
    pub paid: Vec<i128>,
    pub status: Status,
}

#[contracttype]
pub enum DataKey {
    NextBillId,
    Bill(u64),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    BillNotFound = 1,
    InvalidAmount = 2,
    InvalidShares = 3,
    NotParticipant = 4,
    AlreadyCompleted = 5,
    AlreadyCancelled = 6,
    NotFullyPaid = 7,
    NotCreator = 8,
}

#[contract]
pub struct SplitPayContract;

#[contractimpl]
impl SplitPayContract {
    pub fn create_bill(
        env: Env,
        creator: Address,
        token: Address,
        title: String,
        total: i128,
        participants: Vec<Address>,
        shares: Vec<i128>,
    ) -> u64 {
        creator.require_auth();

        if total <= 0 || participants.len() == 0 || participants.len() != shares.len() {
            panic_with_error!(&env, Error::InvalidShares);
        }

        let mut sum: i128 = 0;
        let mut paid: Vec<i128> = Vec::new(&env);

        for i in 0..shares.len() {
            let share = shares.get(i).unwrap();

            if share <= 0 {
                panic_with_error!(&env, Error::InvalidShares);
            }

            sum += share;
            paid.push_back(0);
        }

        if sum != total {
            panic_with_error!(&env, Error::InvalidShares);
        }

        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextBillId)
            .unwrap_or(1);

        env.storage().instance().set(&DataKey::NextBillId, &(id + 1));

        let bill = Bill {
            id,
            creator: creator.clone(),
            token,
            title,
            total,
            participants,
            shares,
            paid,
            status: Status::Pending,
        };

        env.storage().persistent().set(&DataKey::Bill(id), &bill);

        env.events().publish((symbol_short!("bill_new"), creator), id);

        id
    }

    pub fn pay_bill(env: Env, bill_id: u64, user: Address, amount: i128) {
        user.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }

        let mut bill = Self::read_bill(&env, bill_id);

        if bill.status == Status::Completed {
            panic_with_error!(&env, Error::AlreadyCompleted);
        }

        if bill.status == Status::Cancelled {
            panic_with_error!(&env, Error::AlreadyCancelled);
        }

        let index = Self::participant_index(&env, &bill, &user);

        let share = bill.shares.get(index).unwrap();
        let already_paid = bill.paid.get(index).unwrap();
        let remaining = share - already_paid;

        if amount > remaining {
            panic_with_error!(&env, Error::InvalidAmount);
        }

        let token_client = token::Client::new(&env, &bill.token);
        token_client.transfer(
            &user,
            &env.current_contract_address(),
            &amount,
        );

        bill.paid.set(index, already_paid + amount);

        if Self::total_paid(&bill) == bill.total {
            bill.status = Status::Completed;
        } else {
            bill.status = Status::PartiallyPaid;
        }

        env.storage().persistent().set(&DataKey::Bill(bill_id), &bill);

        env.events().publish((symbol_short!("paid"), bill_id, user), amount);
    }

    pub fn claim_bill(env: Env, bill_id: u64, creator: Address) {
        creator.require_auth();

        let bill = Self::read_bill(&env, bill_id);

        if bill.creator != creator {
            panic_with_error!(&env, Error::NotCreator);
        }

        if bill.status == Status::Cancelled {
            panic_with_error!(&env, Error::AlreadyCancelled);
        }

        if Self::total_paid(&bill) != bill.total {
            panic_with_error!(&env, Error::NotFullyPaid);
        }

        let token_client = token::Client::new(&env, &bill.token);
        token_client.transfer(
            &env.current_contract_address(),
            &creator,
            &bill.total,
        );

        env.events().publish((symbol_short!("claim"), bill_id, creator), bill.total);
    }

    pub fn cancel_bill(env: Env, bill_id: u64, creator: Address) {
        creator.require_auth();

        let mut bill = Self::read_bill(&env, bill_id);

        if bill.creator != creator {
            panic_with_error!(&env, Error::NotCreator);
        }

        if bill.status == Status::Completed {
            panic_with_error!(&env, Error::AlreadyCompleted);
        }

        bill.status = Status::Cancelled;

        env.storage().persistent().set(&DataKey::Bill(bill_id), &bill);

        env.events().publish((symbol_short!("cancel"), creator), bill_id);
    }

    pub fn get_bill(env: Env, bill_id: u64) -> Bill {
        Self::read_bill(&env, bill_id)
    }

    fn read_bill(env: &Env, bill_id: u64) -> Bill {
        env.storage()
            .persistent()
            .get(&DataKey::Bill(bill_id))
            .unwrap_or_else(|| panic_with_error!(env, Error::BillNotFound))
    }

    fn participant_index(env: &Env, bill: &Bill, user: &Address) -> u32 {
        for i in 0..bill.participants.len() {
            if bill.participants.get(i).unwrap() == *user {
                return i;
            }
        }

        panic_with_error!(env, Error::NotParticipant);
    }

    fn total_paid(bill: &Bill) -> i128 {
        let mut total: i128 = 0;

        for i in 0..bill.paid.len() {
            total += bill.paid.get(i).unwrap();
        }

        total
    }
}