use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub trait RecoverMutex<T: ?Sized> {
    fn lock_recover(&self) -> MutexGuard<'_, T>;
}

pub trait RecoverRwLock<T: ?Sized> {
    fn read_recover(&self) -> RwLockReadGuard<'_, T>;

    fn write_recover(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T: ?Sized> RecoverMutex<T> for Mutex<T> {
    fn lock_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<T: ?Sized> RecoverRwLock<T> for RwLock<T> {
    fn read_recover(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write_recover(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn poisoned_mutex(value: u32) -> Arc<Mutex<u32>> {
        let lock = Arc::new(Mutex::new(0u32));
        let holder = Arc::clone(&lock);
        let outcome = std::thread::spawn(move || {
            let mut guard = holder.lock().unwrap();
            *guard = value;
            panic!("holder panics while locked");
        })
        .join();
        assert!(outcome.is_err());
        lock
    }

    #[test]
    fn mutex_serves_the_value_left_by_a_panicking_holder() {
        let lock = poisoned_mutex(7);

        assert!(lock.lock().is_err());
        assert_eq!(*lock.lock_recover(), 7);
    }

    #[test]
    fn rwlock_accepts_writes_and_serves_reads_after_a_panicking_holder() {
        let lock = Arc::new(RwLock::new(1u32));
        let holder = Arc::clone(&lock);
        let outcome = std::thread::spawn(move || {
            let _guard = holder.write().unwrap();
            panic!("holder panics while locked");
        })
        .join();
        assert!(outcome.is_err());
        assert!(lock.read().is_err());

        *lock.write_recover() = 9;

        assert_eq!(*lock.read_recover(), 9);
    }
}
