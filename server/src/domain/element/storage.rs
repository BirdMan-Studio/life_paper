use super::definition::ElementId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 生物或世界持有的物质数量。
///
/// 库存只保存稳定 ID 和游戏逻辑数量；名称、密度等定义统一从注册表查询。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ElementStorage {
    amounts: BTreeMap<ElementId, u64>,
}

impl ElementStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_pairs(pairs: impl IntoIterator<Item = (ElementId, u64)>) -> Self {
        let mut storage = Self::new();
        for (element, amount) in pairs {
            storage.set(element, amount);
        }
        storage
    }

    pub fn get(&self, element: &str) -> u64 {
        self.amounts.get(element).copied().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.amounts.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ElementId, u64)> {
        self.amounts
            .iter()
            .map(|(element, &amount)| (element, amount))
    }

    pub fn set(&mut self, element: ElementId, amount: u64) {
        if amount == 0 {
            self.amounts.remove(&element);
        } else {
            self.amounts.insert(element, amount);
        }
    }

    pub fn add(&mut self, element: ElementId, amount: u64) {
        let current = self.get(element.as_str());
        self.set(element, current.saturating_add(amount));
    }

    /// 尝试消耗物质。库存不足时不会改变库存并返回 `false`。
    pub fn try_remove(&mut self, element: &str, amount: u64) -> bool {
        let current = self.get(element);
        if current < amount {
            return false;
        }

        if current == amount {
            self.amounts.remove(element);
        } else if let Some(stored_amount) = self.amounts.get_mut(element) {
            *stored_amount = current - amount;
        }
        true
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ElementRecipe {
    pub inputs: ElementStorage,
    pub outputs: ElementStorage,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::create_default_element_registry;

    #[test]
    fn uses_registered_element_ids() {
        let water = create_default_element_registry()
            .unwrap()
            .get("water")
            .unwrap()
            .id
            .clone();
        let mut storage = ElementStorage::new();

        storage.add(water, 3);
        assert_eq!(storage.get("water"), 3);
        assert!(!storage.try_remove("water", 4));
        assert!(storage.try_remove("water", 2));
        assert_eq!(storage.get("water"), 1);
    }
}
