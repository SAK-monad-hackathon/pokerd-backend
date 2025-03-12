use alloy::sol;

sol! {
    #[sol(rpc)]
    interface IPokerTable {
        error BigBlindPriceIsTooLow(uint256 price);
        error TableIsFull();
        error NotAPlayer();
        error SkippingPhasesIsNotAllowed();
        error InvalidState(GamePhases current, GamePhases required);
        error NotEnoughPlayers();
        error InvalidBuyIn();
        error OccupiedSeat();
        error NotTurnOfPlayer();
        error PlayerStillPlaying();
        error PlayerNotInHand();
        error BetTooSmall();
        error InvalidBetAmount();
        error NotEnoughBalance();
        error InvalidShowdownResults();

        event PlayerJoined(address indexed player, uint256 buyIn, uint256 indexOnTable, GamePhases currentPhase);
        event PlayerLeft(address indexed player, uint256 amountWithdrawn, uint256 indexOnTable, GamePhases currentPhase);
        event PhaseChanged(GamePhases previousPhase, GamePhases newPhase);
        event PlayerBet(address indexed player, uint256 indexOnTable, uint256 betAmount);
        event PlayerFolded(uint256 indexOnTable);
        event PlayerWonWithoutShowdown(address indexed winner, uint256 indexOnTable, uint256 pot, GamePhases phase);
        event ShowdownEnded(PlayerResult[] playersData, uint256 pot, string communityCards);

        #[derive(Debug)]
        enum GamePhases {
            WaitingForPlayers,
            WaitingForDealer,
            PreFlop,
            WaitingForFlop,
            Flop,
            WaitingForTurn,
            Turn,
            WaitingForRiver,
            River,
            WaitingForResult
        }

        struct PlayerResult {
            int256 gains;
            string cards;
        }

        struct RoundData {
            string communityCards;
            PlayerResult[] results;
        }


        function currentRoundId() external returns (uint256 round);
        function currentPhase() external returns (GamePhases phase);
        function isPlayerIndexInRound(uint256 index) external view returns (bool inRound);
        function playerIndices(uint256 index) external view returns (address player);
        function setCurrentPhase(GamePhases newPhase, string calldata cardsToReveal) external;
        function revealShowdownResult(string[] calldata cards, uint256[] calldata winners) external;
        function timeoutCurrentPlayer() external;
        function cancelCurrentRound() external;
    }
}
