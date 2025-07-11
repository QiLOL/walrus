// Copyright (c) Walrus Foundation
// SPDX-License-Identifier: Apache-2.0

/// Module: `subsidies`
///
/// Module to manage a shared subsidy pool, allowing for discounted
/// storage costs for buyers and contributing to a subsidy for storage nodes.
/// It provides functionality to:
///  - Add funds to the shared subsidy pool.
///  - Set subsidy rates for buyers and storage nodes.
///  - Apply subsidies when reserving storage or extending blob lifetimes.
#[deprecated(note = b"This module is superseded by the walrus_subsidies module")]
module subsidies::subsidies;

use std::type_name;
use sui::{balance::{Self, Balance}, coin::Coin, hex};
use wal::wal::WAL;
use walrus::{blob::Blob, storage_resource::Storage, system::System};

// === Constants ===

/// Subsidy rate is in basis points (1/100 of a percent).
const MAX_SUBSIDY_RATE: u16 = 10_000; // 100%

// === Versioning ===

// Whenever the package is upgraded, we create a new type here that will have the ID of the new
// package in its type name. We can then use this to migrate the object to the new package ID
// without requiring the AdminCap.

/// The current version of this contract.
const VERSION: u64 = 3;

/// Helper struct to get the package ID for the version 3 of this contract.
public struct V3()

/// Returns the package ID for the current version of this contract.
/// Needs to be updated whenever the package is upgraded.
fun package_id_for_current_version(): ID {
    package_id_for_type<V3>()
}

/// Returns the package ID for the given type.
fun package_id_for_type<T>(): ID {
    let address_str = type_name::get<T>().get_address().to_lowercase();
    let address_bytes = hex::decode(address_str.into_bytes());
    object::id_from_bytes(address_bytes)
}

// === Errors ===

/// The provided subsidy rate is invalid.
const EInvalidSubsidyRate: u64 = 0;
/// The admin cap is not authorized for the `Subsidies` object.
const EUnauthorizedAdminCap: u64 = 1;
/// The package version is not compatible with the `Subsidies` object.
const EWrongVersion: u64 = 2;

// === Structs ===

/// Capability to perform admin operations, tied to a specific Subsidies object.
///
/// Only the holder of this capability can modify subsidy rates.
public struct AdminCap has key, store {
    id: UID,
    subsidies_id: ID,
}

/// Subsidy rates are expressed in basis points (1/100 of a percent).
/// A subsidy rate of 100 basis points means a 1% subsidy.
/// The maximum subsidy rate is 10,000 basis points (100%).
public struct Subsidies has key, store {
    id: UID,
    /// The subsidy rate applied to the buyer at the moment of storage purchase
    /// in basis points.
    buyer_subsidy_rate: u16,
    /// The subsidy rate applied to the storage node when buying storage in basis
    /// points.
    system_subsidy_rate: u16,
    /// The balance of funds available in the subsidy pool.
    subsidy_pool: Balance<WAL>,
    /// Package ID of the subsidies contract.
    package_id: ID,
    /// The version of the subsidies contract.
    version: u64,
}

/// Creates a new `Subsidies` object and an `AdminCap`.
public fun new(package_id: ID, ctx: &mut TxContext): AdminCap {
    let subsidies = Subsidies {
        id: object::new(ctx),
        buyer_subsidy_rate: 0,
        system_subsidy_rate: 0,
        package_id,
        subsidy_pool: balance::zero(),
        version: VERSION,
    };
    let admin_cap = AdminCap { id: object::new(ctx), subsidies_id: object::id(&subsidies) };
    transfer::share_object(subsidies);
    admin_cap
}

/// Creates a new `Subsidies` object with initial rates and funds and an `AdminCap`.
public fun new_with_initial_rates_and_funds(
    package_id: ID,
    initial_buyer_subsidy_rate: u16,
    initial_system_subsidy_rate: u16,
    initial_funds: Coin<WAL>,
    ctx: &mut TxContext,
): AdminCap {
    assert!(initial_buyer_subsidy_rate <= MAX_SUBSIDY_RATE, EInvalidSubsidyRate);
    assert!(initial_system_subsidy_rate <= MAX_SUBSIDY_RATE, EInvalidSubsidyRate);
    let subsidies = Subsidies {
        id: object::new(ctx),
        buyer_subsidy_rate: initial_buyer_subsidy_rate,
        system_subsidy_rate: initial_system_subsidy_rate,
        subsidy_pool: initial_funds.into_balance(),
        package_id,
        version: VERSION,
    };
    let admin_cap = AdminCap { id: object::new(ctx), subsidies_id: object::id(&subsidies) };
    transfer::share_object(subsidies);
    admin_cap
}

/// Add additional funds to the subsidy pool.
///
/// These funds will be used to provide discounts for buyers
/// and rewards to storage nodes.
public fun add_funds(self: &mut Subsidies, funds: Coin<WAL>) {
    self.subsidy_pool.join(funds.into_balance());
}

/// Check if the admin cap is valid for this subsidies object.
///
/// Aborts if the cap does not match.
fun check_admin(self: &Subsidies, admin_cap: &AdminCap) {
    assert!(object::id(self) == admin_cap.subsidies_id, EUnauthorizedAdminCap);
}

fun check_version_upgrade(self: &Subsidies) {
    assert!(self.version < VERSION, EWrongVersion);
}

/// Set the subsidy rate for buyers, in basis points.
///
/// Aborts if new_rate is greater than the max value.
public fun set_buyer_subsidy_rate(self: &mut Subsidies, cap: &AdminCap, new_rate: u16) {
    check_admin(self, cap);
    assert!(new_rate <= MAX_SUBSIDY_RATE, EInvalidSubsidyRate);
    self.buyer_subsidy_rate = new_rate;
}

/// Allows the admin to withdraw all funds from the subsidy pool.
///
/// This is used to migrate funds from the `Subsidies` object to the `WalrusSubsidies` object in a
/// PTB.
public fun withdraw_balance(self: &mut Subsidies, cap: &AdminCap): Balance<WAL> {
    check_admin(self, cap);
    self.subsidy_pool.withdraw_all()
}

/// Set the subsidy rate for storage nodes, in basis points.
///
/// Aborts if new_rate is greater than the max value.
public fun set_system_subsidy_rate(self: &mut Subsidies, cap: &AdminCap, new_rate: u16) {
    check_admin(self, cap);
    assert!(new_rate <= MAX_SUBSIDY_RATE, EInvalidSubsidyRate);
    self.system_subsidy_rate = new_rate;
}

fun allocate_subsidies(self: &Subsidies, cost: u64, initial_pool_value: u64): (u64, u64) {
    // Return early if the subsidy pool is empty.
    if (initial_pool_value == 0) {
        return (0, 0)
    };
    let buyer_subsidy = cost * (self.buyer_subsidy_rate as u64) / (MAX_SUBSIDY_RATE as u64);
    let system_subsidy = cost * (self.system_subsidy_rate as u64) / (MAX_SUBSIDY_RATE as u64);
    let total_subsidy = buyer_subsidy + system_subsidy;

    // Apply subsidy up to the available amount in the pool.
    if (initial_pool_value >= total_subsidy) {
        (buyer_subsidy, system_subsidy)
    } else {
        // If we don't have enough in the pool to pay the full subsidies,
        // split the remainder proportionally between the buyer and system subsidies.
        let pool_value = initial_pool_value;
        let total_subsidy_rate = self.buyer_subsidy_rate + self.system_subsidy_rate;
        let buyer_subsidy =
            pool_value * (self.buyer_subsidy_rate as u64) / (total_subsidy_rate as u64);
        let system_subsidy =
            pool_value * (self.system_subsidy_rate as u64) / (total_subsidy_rate as u64);
        (buyer_subsidy, system_subsidy)
    }
}

#[allow(lint(coin_field))]
public struct CombinedPayment {
    payment: Coin<WAL>,
    initial_payment_value: u64,
    initial_pool_value: u64,
}

/// Combine buyer payment with subsidy pool to form a CombinedPayment
fun combine_payment_with_pool(
    self: &mut Subsidies,
    payment: &mut Coin<WAL>,
    ctx: &mut TxContext,
): CombinedPayment {
    let initial_payment_value = payment.value();
    let mut pool_coin = self.subsidy_pool.withdraw_all().into_coin(ctx);
    let initial_pool_value = pool_coin.value();
    let buyer_balance = payment.balance_mut().split(initial_payment_value);

    pool_coin.balance_mut().join(buyer_balance);

    CombinedPayment {
        payment: pool_coin,
        initial_payment_value,
        initial_pool_value,
    }
}

/// Applies subsidies and sends rewards to the system.
///
/// This will send WAL back to the buyer coin from the subsidy pool,
/// and send the storage node subsidy to the system.
fun handle_subsidies_and_payment(
    self: &mut Subsidies,
    system: &mut System,
    combined_payment_after: CombinedPayment,
    buyer_payment: &mut Coin<WAL>,
    epochs_ahead: u32,
    ctx: &mut TxContext,
) {
    let CombinedPayment { payment: remaining_coin, initial_payment_value, initial_pool_value } =
        combined_payment_after;
    // Calculate cost
    let initial_combined_value = initial_pool_value + initial_payment_value;
    let cost = initial_combined_value - remaining_coin.value();

    // Refund buyer
    self.subsidy_pool.join(remaining_coin.into_balance());
    let (buyer_subsidy, system_subsidy) = self.allocate_subsidies(cost, initial_pool_value);
    let buyer_coin_value = initial_payment_value + buyer_subsidy - cost;
    let buyer_refund = self.subsidy_pool.split(buyer_coin_value);
    buyer_payment.balance_mut().join(buyer_refund);

    // Subsidize system
    let system_subsidy_coin = self.subsidy_pool.split(system_subsidy).into_coin(ctx);
    system.add_subsidy(system_subsidy_coin, epochs_ahead);
    assert!(buyer_payment.value() <= initial_payment_value);
}

/// Extends a blob's lifetime and applies the buyer and storage node subsidies.
///
/// It first extends the blob lifetime using system `extend_blob` method.
/// Then it applies the subsidies and deducts the funds from the subsidy pool.
public fun extend_blob(
    self: &mut Subsidies,
    system: &mut System,
    blob: &mut Blob,
    epochs_ahead: u32,
    payment: &mut Coin<WAL>,
    ctx: &mut TxContext,
) {
    assert!(self.version == VERSION, EWrongVersion);
    if (self.subsidy_pool.value() == 0) {
        return system.extend_blob(blob, epochs_ahead, payment)
    };
    let mut combined_payment = self.combine_payment_with_pool(payment, ctx);

    system.extend_blob(blob, epochs_ahead, &mut combined_payment.payment);

    handle_subsidies_and_payment(
        self,
        system,
        combined_payment,
        payment,
        epochs_ahead,
        ctx,
    );
}

/// Reserves storage space and applies the buyer and storage node subsidies.
///
/// It first reserves the space using system `reserve_space` method.
/// Then it applies the subsidies and deducts the funds from the subsidy pool.
public fun reserve_space(
    self: &mut Subsidies,
    system: &mut System,
    storage_amount: u64,
    epochs_ahead: u32,
    payment: &mut Coin<WAL>,
    ctx: &mut TxContext,
): Storage {
    assert!(self.version == VERSION, EWrongVersion);
    if (self.subsidy_pool.value() == 0) {
        return system.reserve_space(storage_amount, epochs_ahead, payment, ctx)
    };
    let mut combined_payment = self.combine_payment_with_pool(payment, ctx);

    let storage = system.reserve_space(
        storage_amount,
        epochs_ahead,
        &mut combined_payment.payment,
        ctx,
    );

    handle_subsidies_and_payment(
        self,
        system,
        combined_payment,
        payment,
        epochs_ahead,
        ctx,
    );
    storage
}

/// Proxy Register blob by calling the system contract
public fun register_blob(
    self: &mut Subsidies,
    system: &mut System,
    storage: Storage,
    blob_id: u256,
    root_hash: u256,
    size: u64,
    encoding_type: u8,
    deletable: bool,
    write_payment: &mut Coin<WAL>,
    ctx: &mut TxContext,
): Blob {
    assert!(self.version == VERSION, EWrongVersion);
    let blob = system.register_blob(
        storage,
        blob_id,
        root_hash,
        size,
        encoding_type,
        deletable,
        write_payment,
        ctx,
    );
    blob
}

entry fun migrate(subsidies: &mut Subsidies) {
    check_version_upgrade(subsidies);
    subsidies.version = VERSION;
    subsidies.package_id = package_id_for_current_version();
}

// === Accessors ===

public fun admin_cap_subsidies_id(admin_cap: &AdminCap): ID {
    admin_cap.subsidies_id
}

/// Returns the current value of the subsidy pool.
public fun subsidy_pool_value(self: &Subsidies): u64 {
    self.subsidy_pool.value()
}

/// Returns the current rate for buyer subsidies.
public fun buyer_subsidy_rate(self: &Subsidies): u16 {
    self.buyer_subsidy_rate
}

/// Returns the current rate for storage node subsidies.
public fun system_subsidy_rate(self: &Subsidies): u16 {
    self.system_subsidy_rate
}

// === Tests ===

#[test_only]
use sui::test_utils::destroy;

#[test_only]
public fun get_subsidy_pool(self: &Subsidies): &Balance<WAL> {
    &self.subsidy_pool
}

#[test_only]
public fun new_for_testing(ctx: &mut TxContext): (Subsidies, AdminCap) {
    let package_id = object::new(ctx);
    let subsidies = Subsidies {
        id: object::new(ctx),
        buyer_subsidy_rate: 0,
        system_subsidy_rate: 0,
        subsidy_pool: balance::zero(),
        package_id: package_id.to_inner(),
        version: VERSION,
    };
    let admin_cap = AdminCap {
        id: object::new(ctx),
        subsidies_id: object::id(&subsidies),
    };
    object::delete(package_id);
    (subsidies, admin_cap)
}

#[test_only]
public fun new_with_initial_rates_and_funds_for_testing(
    initial_buyer_subsidy_rate: u16,
    initial_system_subsidy_rate: u16,
    initial_funds: Coin<WAL>,
    ctx: &mut TxContext,
): (Subsidies, AdminCap) {
    assert!(initial_buyer_subsidy_rate <= MAX_SUBSIDY_RATE, EInvalidSubsidyRate);
    assert!(initial_system_subsidy_rate <= MAX_SUBSIDY_RATE, EInvalidSubsidyRate);
    let package_id = object::new(ctx);
    let subsidies = Subsidies {
        id: object::new(ctx),
        buyer_subsidy_rate: initial_buyer_subsidy_rate,
        system_subsidy_rate: initial_system_subsidy_rate,
        subsidy_pool: initial_funds.into_balance(),
        version: VERSION,
        package_id: package_id.to_inner(),
    };
    let admin_cap = AdminCap {
        id: object::new(ctx),
        subsidies_id: object::id(&subsidies),
    };
    object::delete(package_id);
    (subsidies, admin_cap)
}

#[test_only]
public fun destroy_admin_cap(admin_cap: AdminCap) {
    destroy(admin_cap);
}

#[test_only]
public fun destroy_subsidies(subsidies: Subsidies) {
    destroy(subsidies);
}
