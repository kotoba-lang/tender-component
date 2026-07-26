(ns tender.component-test
  (:require [clojure.test :refer [deftest is testing]]
            [tender.component :as component]))

(deftest native-ability-protocol-preserves-qualified-operation
  (testing "the Rust micro-TCB receives the canonical WIT operation identity"
    (is (= {:target "clock://monotonic"
            :operation "clock/now"
            :max-bytes 1
            :max-items 1
            :deadline-ms 10
            :audit-id "serialization-test"}
           (#'component/host-ability
            {:target "clock://monotonic"
             :operation :clock/now
             :max-bytes 1
             :max-items 1
             :deadline-ms 10
             :audit-id "serialization-test"}))))
  (testing "an unqualified operation fails closed"
    (is (thrown? clojure.lang.ExceptionInfo
                 (#'component/host-ability {:operation :now})))))
