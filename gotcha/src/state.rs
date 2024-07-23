pub trait ExtendableState<NewState> {
    type Ret;
    fn extend(self, next: NewState) -> Self::Ret;
}

impl<NewState> ExtendableState<NewState> for () {
    type Ret = (NewState,);

    fn extend(self, next: NewState) -> Self::Ret {
        (next,)
    }
}

impl<T, NewState> ExtendableState<NewState> for (T,) {
    type Ret = (T, NewState);

    fn extend(self, next: NewState) -> Self::Ret {
        (self.0, next)
    }
}

impl<T1, T2, NewState> ExtendableState<NewState> for (T1, T2) {
    type Ret = (T1, T2, NewState);
    fn extend(self, next: NewState) -> Self::Ret {
        (self.0, self.1, next)
    }
}

impl<T1, T2, T3, NewState> ExtendableState<NewState> for (T1, T2, T3) {
    type Ret = (T1, T2, T3, NewState);
    fn extend(self, next: NewState) -> Self::Ret {
        (self.0, self.1, self.2, next)
    }
}
