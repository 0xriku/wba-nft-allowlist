use crate::msg::{AdminListResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use anyhow::{anyhow, Result};
use assert_matches::assert_matches;
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{to_binary, Addr, CosmosMsg, Empty, QueryRequest, StdError, WasmMsg, WasmQuery, Uint128, Coin};
use cw1::Cw1Contract;
use cw_multi_test::{App, AppBuilder, AppResponse, BankKeeper, Contract, ContractWrapper, Executor};
use derivative::Derivative;
use serde::{de::DeserializeOwned, Serialize};

use nft::{self, helpers::NftContract};

const USER: &str = "USER";
const ADMIN: &str = "ADMIN";
const NATIVE_DENOM: &str = "denom";

fn mock_app() -> App {
    AppBuilder::new().build(|router, _, storage| {
        router
            .bank
            .init_balance(
                storage,
                &Addr::unchecked(USER),
                vec![Coin {
                    denom: NATIVE_DENOM.to_string(),
                    amount: Uint128::new(1),
                }],
            )
            .unwrap();
    })
}

pub fn contract_cw1() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );
    Box::new(contract)
}

pub fn contract_nft() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        nft::contract::entry::execute,
        nft::contract::entry::instantiate,
        nft::contract::entry::query,
    );
    Box::new(contract)
}


#[derive(Derivative)]
#[derivative(Debug)]
pub struct Suite {
    /// Application mock
    #[derivative(Debug = "ignore")]
    app: App,
    /// Special account
    pub owner: String,
    /// ID of stored code for cw1 contract
    cw1_id: u64,

    nft_id:u64
}

impl Suite {
    pub fn init() -> Result<Suite> {
        let mut app = mock_app();
        let owner = "owner".to_owned();
        let cw1_id = app.store_code(contract_cw1());
        let nft_id = app.store_code(contract_nft());

        Ok(Suite { app, owner, cw1_id, nft_id })
    }

    pub fn instantiate_cw1_contract(&mut self, admins: Vec<String>, mutable: bool) -> Cw1Contract {
        let contract = self
            .app
            .instantiate_contract(
                self.cw1_id,
                Addr::unchecked(self.owner.clone()),
                &InstantiateMsg { admins, mutable },
                &[],
                "Whitelist",
                None,
            )
            .unwrap();
        Cw1Contract(contract)
    }

    pub fn instantiate_nft_contract(&mut self, name:String, symbol:String, minter:String) -> NftContract {
        let contract = self
            .app
            .instantiate_contract(
                self.nft_id,
                Addr::unchecked(self.owner.clone()),
                &nft::contract::InstantiateMsg { name, symbol, minter },
                &[],
                "nft",
                None,
            )
            .unwrap();
        NftContract(contract)
    }

    pub fn execute<M>(
        &mut self,
        sender_contract: Addr,
        target_contract: &Addr,
        msg: M,
    ) -> Result<AppResponse>
    where
        M: Serialize + DeserializeOwned,
    {
        let execute: ExecuteMsg = ExecuteMsg::Execute {
            msgs: vec![CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: target_contract.to_string(),
                msg: to_binary(&msg)?,
                funds: vec![],
            })],
        };
        self.app
            .execute_contract(
                Addr::unchecked("addr1".to_string()),
                sender_contract,
                &execute,
                &[],
            )
            .map_err(|err| anyhow!(err))
    }

    pub fn query<M>(&self, target_contract: Addr, msg: M) -> Result<AdminListResponse, StdError>
    where
        M: Serialize + DeserializeOwned,
    {
        self.app.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: target_contract.to_string(),
            msg: to_binary(&msg).unwrap(),
        }))
    }
}

#[test]
fn make_nft_contract_owned_by_cw1_then_mint() {
    let mut suite = Suite::init().unwrap();
    let cw1_contract = suite.instantiate_cw1_contract(vec!["addr1".to_string(), "addr2".to_string(), "addr3".to_string()], false);
    let nft_contract = suite.instantiate_nft_contract("test_nft".to_string(), "TEST".to_string(), cw1_contract.addr().to_string());
    let mint_msg = nft::contract::MintMsg{token_id:"0".to_string(), owner:USER.to_string(), token_uri:Some("url".to_string()), extension:None };
    let msg = nft::contract::ExecuteMsg::Mint(mint_msg);
    suite.execute(cw1_contract.addr(), &nft_contract.addr(), msg).unwrap();
}

#[test]
fn proxy_freeze_message() {
    let mut suite = Suite::init().unwrap();

    let first_contract = suite.instantiate_cw1_contract(vec![suite.owner.clone()], true);
    let second_contract =
        suite.instantiate_cw1_contract(vec![first_contract.addr().to_string()], true);
    assert_ne!(second_contract, first_contract);

    let freeze_msg: ExecuteMsg = ExecuteMsg::Freeze {};
    assert_matches!(
        suite.execute(first_contract.addr(), &second_contract.addr(), freeze_msg),
        Ok(_)
    );

    let query_msg: QueryMsg = QueryMsg::AdminList {};
    assert_matches!(
        suite.query(second_contract.addr(), query_msg),
        Ok(
            AdminListResponse {
                mutable,
                ..
            }) => {
            assert!(!mutable)
        }
    );
}
