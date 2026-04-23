use rubik::{
    Byte, CenterReductionStage, CornerReductionStage, CornerSearchStage, CornerTwoCycleStage,
    EdgePairingStage, ExecutionMode, SolverStage,
};

fn assert_default_stage_contract_consistency<T>(stage: &T)
where
    T: SolverStage<Byte>,
{
    let contract = <T as SolverStage<Byte>>::contract(stage);

    for side_length in 1..=5 {
        assert_eq!(
            <T as SolverStage<Byte>>::is_applicable_to_side_length(stage, side_length),
            contract.side_lengths.supports(side_length),
            "side-length applicability must match the declared contract for n={side_length}",
        );
        assert_eq!(
            contract.supports(side_length, ExecutionMode::Standard),
            contract.side_lengths.supports(side_length),
            "standard mode support must match side-length support for n={side_length}",
        );
        assert_eq!(
            contract.supports(side_length, ExecutionMode::Optimized),
            contract.side_lengths.supports(side_length)
                && <T as SolverStage<Byte>>::execution_mode_support(stage)
                    .supports(ExecutionMode::Optimized),
            "optimized mode support must match declared execution support for n={side_length}",
        );
    }

    assert!(!contract.standard_preconditions.is_empty());
    assert!(!contract.standard_postconditions.is_empty());
}

#[test]
fn public_stage_contract_api_is_consistent_for_top_level_stages() {
    assert_default_stage_contract_consistency(&CenterReductionStage::western_default());
    assert_default_stage_contract_consistency(&CornerReductionStage::default());
    assert_default_stage_contract_consistency(&EdgePairingStage::default());
}

#[test]
fn named_corner_strategy_stage_contracts_are_consistent() {
    assert_default_stage_contract_consistency(&CornerSearchStage::default());
    assert_default_stage_contract_consistency(&CornerTwoCycleStage::default());
}
