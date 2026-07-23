use embedded_storage::nor_flash::NorFlash;

use crate::NvsKey;

#[derive(Debug)]
pub enum NvsError<K :NvsKey, T: NorFlash>
{
    MissingPageData,
    MissingState,
    MissingCacheData,
    InconsistentSize(u16),
    DataTooBig(usize),
    MissingKey(K),
    DuplicateKey(K),
    Flash(T::Error)
}

macro_rules! map_err {
    {$err:expr} => {
        $err.map_err(|e| NvsError::Flash(e))
    };
}
pub(crate) use map_err;

// impl<K :NvsKey, T: NorFlash> From<T::Error> for NvsError<K, T>
// {
//     fn from(value: T::Error) -> Self
//     {
//         return Self::Flash(value);
//     }
// }